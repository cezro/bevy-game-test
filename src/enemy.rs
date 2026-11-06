use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyType {
    Runner,
    Ninja,
    Heavy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnemyState {
    #[allow(dead_code)]
    Spawning { timer: f32 },
    Chase,
    Windup { timer: f32, duration: f32 },
    Attack { timer: f32, duration: f32, has_hit: bool },
    #[allow(dead_code)]
    Hitstun { timer: f32 },
}

#[derive(Component, Debug, Clone)]
pub struct Enemy {
    pub enemy_type: EnemyType,
    pub state: EnemyState,
    pub velocity: Vec2,
    pub facing: f32,
    pub is_grounded: bool,
    #[allow(dead_code)]
    pub health: f32,
    #[allow(dead_code)]
    pub max_health: f32,
    pub attack_range: f32,
    pub speed: f32,
    pub jump_force: f32,
    pub hitbox_radius: f32,
    pub attack_damage: f32,
}

impl Enemy {
    pub fn new(enemy_type: EnemyType) -> Self {
        match enemy_type {
            EnemyType::Runner => Self {
                enemy_type,
                state: EnemyState::Chase,
                velocity: Vec2::ZERO,
                facing: 1.0,
                is_grounded: true,
                health: 1.0, // 1-hit kill slicing
                max_health: 1.0,
                attack_range: 52.0,
                speed: 210.0,
                jump_force: 500.0,
                hitbox_radius: 18.0,
                attack_damage: 15.0,
            },
            EnemyType::Ninja => Self {
                enemy_type,
                state: EnemyState::Chase,
                velocity: Vec2::ZERO,
                facing: 1.0,
                is_grounded: true,
                health: 1.0,
                max_health: 1.0,
                attack_range: 65.0,
                speed: 280.0,
                jump_force: 620.0,
                hitbox_radius: 16.0,
                attack_damage: 20.0,
            },
            EnemyType::Heavy => Self {
                enemy_type,
                state: EnemyState::Chase,
                velocity: Vec2::ZERO,
                facing: 1.0,
                is_grounded: true,
                health: 2.0,
                max_health: 2.0,
                attack_range: 80.0,
                speed: 130.0,
                jump_force: 420.0,
                hitbox_radius: 26.0,
                attack_damage: 35.0,
            },
        }
    }
}

/// Pure AI step function: calculates next state and intended horizontal motion.
pub fn step_enemy_ai(
    enemy_pos: Vec2,
    player_pos: Vec2,
    state: &EnemyState,
    enemy_type: EnemyType,
    attack_range: f32,
    is_grounded: bool,
    dt: f32,
) -> (EnemyState, f32, bool) {
    let to_player = player_pos - enemy_pos;
    let dist_x = to_player.x.abs();
    let dist_y = to_player.y;
    let facing = if to_player.x >= 0.0 { 1.0 } else { -1.0 };
    let mut should_jump = false;
    let mut move_dir = 0.0;

    let next_state = match state {
        EnemyState::Spawning { timer } => {
            let rem = timer - dt;
            if rem <= 0.0 {
                EnemyState::Chase
            } else {
                EnemyState::Spawning { timer: rem }
            }
        }
        EnemyState::Chase => {
            // Check if in attack distance
            if dist_x <= attack_range && dist_y.abs() <= 50.0 {
                let windup_time = match enemy_type {
                    EnemyType::Runner => 0.45,
                    EnemyType::Ninja => 0.28,
                    EnemyType::Heavy => 0.70,
                };
                EnemyState::Windup {
                    timer: windup_time,
                    duration: windup_time,
                }
            } else {
                move_dir = facing;
                // Jump if player is on a higher platform and we're somewhat below them
                if is_grounded && dist_y > 45.0 && dist_x < 180.0 {
                    should_jump = true;
                }
                EnemyState::Chase
            }
        }
        EnemyState::Windup { timer, duration } => {
            let rem = timer - dt;
            if rem <= 0.0 {
                let attack_time = match enemy_type {
                    EnemyType::Runner => 0.25,
                    EnemyType::Ninja => 0.20,
                    EnemyType::Heavy => 0.35,
                };
                EnemyState::Attack {
                    timer: attack_time,
                    duration: attack_time,
                    has_hit: false,
                }
            } else {
                EnemyState::Windup {
                    timer: rem,
                    duration: *duration,
                }
            }
        }
        EnemyState::Attack {
            timer,
            duration,
            has_hit,
        } => {
            let rem = timer - dt;
            // Slight lunge forward during attack
            move_dir = facing * 0.6;
            if rem <= 0.0 {
                EnemyState::Chase
            } else {
                EnemyState::Attack {
                    timer: rem,
                    duration: *duration,
                    has_hit: *has_hit,
                }
            }
        }
        EnemyState::Hitstun { timer } => {
            let rem = timer - dt;
            if rem <= 0.0 {
                EnemyState::Chase
            } else {
                EnemyState::Hitstun { timer: rem }
            }
        }
    };

    (next_state, move_dir, should_jump)
}

/// Pure wave scaling calculation: returns total enemy count for given wave number.
pub fn calculate_wave_enemy_count(wave: u32) -> u32 {
    3 + wave * 3
}

/// Wave Manager state
#[derive(Debug, Clone, PartialEq)]
pub enum WavePhase {
    Intermission { timer: f32 },
    Spawning { remaining_to_spawn: u32, spawn_timer: f32 },
    Active,
    WaveCleared { timer: f32 },
}

#[derive(Resource, Debug, Clone)]
pub struct WaveManager {
    pub current_wave: u32,
    pub phase: WavePhase,
    pub total_wave_enemies: u32,
    pub alive_count: u32,
    pub spawn_interval: f32,
    pub intermission_duration: f32,
}

impl Default for WaveManager {
    fn default() -> Self {
        Self {
            current_wave: 1,
            phase: WavePhase::Intermission { timer: 2.0 },
            total_wave_enemies: 6,
            alive_count: 0,
            spawn_interval: 1.2,
            intermission_duration: 2.5,
        }
    }
}

pub enum WaveAction {
    SpawnEnemy(EnemyType, Vec2),
    #[allow(dead_code)]
    AnnounceWave(u32),
    #[allow(dead_code)]
    AnnounceClear(u32),
}

impl WaveManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pure update step for wave progression. Returns an optional action.
    pub fn update(&mut self, dt: f32, arena_width: f32, spawn_y: f32) -> Option<WaveAction> {
        match &mut self.phase {
            WavePhase::Intermission { timer } => {
                *timer -= dt;
                if *timer <= 0.0 {
                    let total = calculate_wave_enemy_count(self.current_wave);
                    self.total_wave_enemies = total;
                    self.phase = WavePhase::Spawning {
                        remaining_to_spawn: total,
                        spawn_timer: 0.1,
                    };
                    return Some(WaveAction::AnnounceWave(self.current_wave));
                }
            }
            WavePhase::Spawning {
                remaining_to_spawn,
                spawn_timer,
            } => {
                *spawn_timer -= dt;
                if *spawn_timer <= 0.0 && *remaining_to_spawn > 0 {
                    *remaining_to_spawn -= 1;
                    *spawn_timer = (self.spawn_interval - (self.current_wave as f32 * 0.05)).max(0.4);

                    // Alternate left / right screen edges
                    let spawn_side = if *remaining_to_spawn % 2 == 0 { -1.0 } else { 1.0 };
                    let spawn_x = spawn_side * (arena_width * 0.5 - 40.0);
                    let pos = Vec2::new(spawn_x, spawn_y);

                    // Choose enemy type based on wave
                    let e_type = if self.current_wave >= 3 && *remaining_to_spawn % 4 == 0 {
                        EnemyType::Heavy
                    } else if self.current_wave >= 2 && *remaining_to_spawn % 3 == 0 {
                        EnemyType::Ninja
                    } else {
                        EnemyType::Runner
                    };

                    self.alive_count += 1;
                    if *remaining_to_spawn == 0 {
                        self.phase = WavePhase::Active;
                    }
                    return Some(WaveAction::SpawnEnemy(e_type, pos));
                }
            }
            WavePhase::Active => {
                if self.alive_count == 0 {
                    self.phase = WavePhase::WaveCleared { timer: 2.0 };
                    return Some(WaveAction::AnnounceClear(self.current_wave));
                }
            }
            WavePhase::WaveCleared { timer } => {
                *timer -= dt;
                if *timer <= 0.0 {
                    self.current_wave += 1;
                    self.phase = WavePhase::Intermission {
                        timer: self.intermission_duration,
                    };
                }
            }
        }
        None
    }

    pub fn on_enemy_killed(&mut self) {
        if self.alive_count > 0 {
            self.alive_count -= 1;
        }
    }
}
