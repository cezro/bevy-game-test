use bevy::prelude::*;

#[derive(Clone, Debug)]
pub struct MovementConfig {
    pub accel: f32,
    pub decel: f32,
    pub max_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
    pub fall_gravity_mult: f32,
    pub short_hop_mult: f32,
    pub coyote_time_max: f32,
    pub jump_buffer_max: f32,
    pub dash_speed: f32,
    pub dash_duration: f32,
    pub dash_cooldown_max: f32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            accel: 2200.0,
            decel: 2800.0,
            max_speed: 340.0,
            jump_force: 580.0,
            gravity: 1400.0,
            fall_gravity_mult: 1.6,
            short_hop_mult: 2.2,
            coyote_time_max: 0.12,
            jump_buffer_max: 0.12,
            dash_speed: 780.0,
            dash_duration: 0.14,
            dash_cooldown_max: 0.40,
        }
    }
}

/// Pure kinematic calculation for horizontal velocity with crisp acceleration and deceleration.
pub fn step_horizontal_velocity(current_vx: f32, input_x: f32, config: &MovementConfig, dt: f32) -> f32 {
    if input_x.abs() > 0.01 {
        let target_vx = input_x.signum() * config.max_speed;
        let diff = target_vx - current_vx;
        let step = config.accel * dt;
        if diff.abs() <= step {
            target_vx
        } else {
            current_vx + diff.signum() * step
        }
    } else {
        // Crisp deceleration / friction
        let step = config.decel * dt;
        if current_vx.abs() <= step {
            0.0
        } else {
            current_vx - current_vx.signum() * step
        }
    }
}

/// Pure kinematic calculation for vertical velocity with coyote time, jump buffer, and gravity scaling.
pub fn step_vertical_velocity(
    current_vy: f32,
    is_grounded: bool,
    jump_held: bool,
    jump_pressed: bool,
    mut coyote_timer: f32,
    mut jump_buffer_timer: f32,
    config: &MovementConfig,
    dt: f32,
) -> (f32, f32, f32, bool) {
    if is_grounded {
        coyote_timer = config.coyote_time_max;
    } else {
        coyote_timer = (coyote_timer - dt).max(0.0);
    }

    if jump_pressed {
        jump_buffer_timer = config.jump_buffer_max;
    } else {
        jump_buffer_timer = (jump_buffer_timer - dt).max(0.0);
    }

    let mut new_vy = current_vy;
    let mut did_jump = false;

    // Check if jump can trigger
    if jump_buffer_timer > 0.0 && coyote_timer > 0.0 {
        new_vy = config.jump_force;
        coyote_timer = 0.0;
        jump_buffer_timer = 0.0;
        did_jump = true;
    } else {
        // Variable gravity scaling
        let effective_gravity = if new_vy < 0.0 {
            config.gravity * config.fall_gravity_mult
        } else if !jump_held && new_vy > 0.0 {
            // Cut jump short if player releases button early
            config.gravity * config.short_hop_mult
        } else {
            config.gravity
        };

        new_vy -= effective_gravity * dt;
    }

    (new_vy, coyote_timer, jump_buffer_timer, did_jump)
}

/// Player Chrono Energy meter component
#[derive(Component, Debug, Clone)]
pub struct ChronoEnergy {
    pub current: f32,
    pub max: f32,
    pub drain_rate: f32,
    pub regen_rate: f32,
    pub teleport_cost: f32,
    pub kill_restore: f32,
}

impl Default for ChronoEnergy {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
            drain_rate: 22.0,
            regen_rate: 14.0,
            teleport_cost: 25.0,
            kill_restore: 25.0,
        }
    }
}

impl ChronoEnergy {
    pub fn can_teleport(&self) -> bool {
        self.current >= self.teleport_cost
    }

    pub fn consume_teleport(&mut self) -> bool {
        if self.can_teleport() {
            self.current = (self.current - self.teleport_cost).max(0.0);
            true
        } else {
            false
        }
    }

    pub fn update(&mut self, is_dilating: bool, dt: f32) {
        if is_dilating {
            self.current = (self.current - self.drain_rate * dt).max(0.0);
        } else {
            self.current = (self.current + self.regen_rate * dt).min(self.max);
        }
    }

    pub fn update_with_drain(&mut self, drain: f32, dt: f32) {
        let drain = if drain.is_finite() { drain.max(0.0) } else { 0.0 };
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if drain > 0.0 {
            self.current = (self.current - drain * dt).max(0.0);
        } else {
            self.current = (self.current + self.regen_rate * dt).min(self.max);
        }
    }

    pub fn reward_kill(&mut self) {
        self.current = (self.current + self.kill_restore).min(self.max);
    }
}

#[derive(Component, Debug)]
pub struct Player {
    pub velocity: Vec2,
    pub facing: f32, // 1.0 = right, -1.0 = left
    pub is_grounded: bool,
    pub coyote_timer: f32,
    pub jump_buffer_timer: f32,
    pub dash_timer: f32,
    pub dash_cooldown: f32,
    pub dash_dir: Vec2,
    pub slash_cooldown: f32,
    pub warp_cooldown: f32,
    pub invulnerability_timer: f32,
    pub health: f32,
    pub max_health: f32,
    pub config: MovementConfig,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            velocity: Vec2::ZERO,
            facing: 1.0,
            is_grounded: true,
            coyote_timer: 0.0,
            jump_buffer_timer: 0.0,
            dash_timer: 0.0,
            dash_cooldown: 0.0,
            dash_dir: Vec2::X,
            slash_cooldown: 0.0,
            warp_cooldown: 0.0,
            invulnerability_timer: 0.0,
            health: 100.0,
            max_health: 100.0,
            config: MovementConfig::default(),
        }
    }
}

/// Static platform definition for level geometry
#[derive(Component, Debug, Clone)]
pub struct Platform {
    pub min_x: f32,
    pub max_x: f32,
    pub y: f32,
    #[allow(dead_code)]
    pub thickness: f32,
}

impl Platform {
    pub fn new(center_x: f32, y: f32, width: f32) -> Self {
        Self {
            min_x: center_x - width * 0.5,
            max_x: center_x + width * 0.5,
            y,
            thickness: 10.0,
        }
    }
}

/// Player setup system: spawns the player entity
#[allow(dead_code)]
pub fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Player::default(),
        ChronoEnergy::default(),
        Transform::from_xyz(0.0, -200.0, 10.0),
        Visibility::default(),
    ));
}
