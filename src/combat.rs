use bevy::prelude::*;

/// Pure mathematical calculation: minimum squared distance from point `p` to line segment `[a, b]`.
pub fn point_to_segment_distance_sq(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-6 {
        return (p - a).length_squared();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    (p - projection).length_squared()
}

/// Pure test: checks if line segment `[a, b]` intersects a circle of given `center` and `radius`.
pub fn segment_intersects_circle(a: Vec2, b: Vec2, center: Vec2, radius: f32) -> bool {
    point_to_segment_distance_sq(center, a, b) <= radius * radius
}

/// Pure test: checks if target circle intersects a katana slash arc.
/// `origin`: player position
/// `facing`: 1.0 (right) or -1.0 (left)
/// `aim_dir`: unit direction towards cursor/attack
/// `target`: enemy position
/// `slash_range`: maximum blade radius
/// `arc_radians`: angular spread of the slash (e.g. 140 degrees = 2.44 rad)
/// `target_radius`: enemy hitbox radius
pub fn katana_arc_intersects(
    origin: Vec2,
    facing: f32,
    aim_dir: Vec2,
    target: Vec2,
    slash_range: f32,
    arc_radians: f32,
    target_radius: f32,
) -> bool {
    let to_target = target - origin;
    let dist = to_target.length();
    if dist > slash_range + target_radius {
        return false;
    }
    // If very close to player, hit unconditionally
    if dist <= target_radius * 1.5 {
        return true;
    }

    let dir = to_target / dist;
    // Base attack direction defaults to facing direction if aim_dir is near zero
    let primary_dir = if aim_dir.length_squared() > 0.01 {
        aim_dir.normalize()
    } else {
        Vec2::new(facing, 0.0)
    };

    let angle_diff = dir.dot(primary_dir).clamp(-1.0, 1.0).acos();
    angle_diff <= (arc_radians * 0.5) + (target_radius / dist.max(1.0))
}

/// Event triggered when an enemy is sliced in combat.
#[derive(Event, Debug, Clone)]
pub struct SliceEnemyEvent {
    pub enemy_entity: Entity,
    pub slice_position: Vec2,
    pub slice_direction: Vec2, // Direction of the cut
    pub impulse: Vec2,         // Force imparted to ragdoll halves
    pub is_warp_kill: bool,
}

/// Visual representation of an active slash arc.
#[derive(Component, Debug, Clone)]
pub struct SlashArcVisual {
    pub origin: Vec2,
    pub angle: f32,
    pub arc_span: f32,
    pub radius: f32,
    pub timer: f32,
    pub duration: f32,
    pub color: Color,
}

/// Visual representation of a Chrono Warp slice beam.
#[derive(Component, Debug, Clone)]
pub struct WarpBeamVisual {
    pub start: Vec2,
    pub end: Vec2,
    pub timer: f32,
    pub duration: f32,
    pub color: Color,
}

/// Bullet projectile fired by gunner enemies.
#[derive(Component, Debug, Clone)]
pub struct Bullet {
    pub velocity: Vec2,
    pub radius: f32,
    pub damage: f32,
    pub lifetime: f32,
    pub is_automatic: bool,
}

/// Combo & Score tracker
#[derive(Resource, Debug, Clone)]
pub struct ComboState {
    pub count: u32,
    pub multiplier: u32,
    pub timer: f32,
    pub window: f32,
    pub score: u64,
    pub highest_combo: u32,
    pub total_kills: u32,
    pub warp_kills: u32,
}

impl Default for ComboState {
    fn default() -> Self {
        Self {
            count: 0,
            multiplier: 1,
            timer: 0.0,
            window: 2.8,
            score: 0,
            highest_combo: 0,
            total_kills: 0,
            warp_kills: 0,
        }
    }
}

impl ComboState {
    /// Registers a kill, resets combo window, updates multiplier and awards score.
    pub fn register_kill(&mut self, is_warp: bool) -> u64 {
        self.count += 1;
        self.total_kills += 1;
        if is_warp {
            self.warp_kills += 1;
        }

        self.highest_combo = self.highest_combo.max(self.count);
        // Multiplier progression: 1x -> 2x (count>=2) -> 3x (count>=5) -> 4x (count>=9) -> 5x (count>=14)...
        self.multiplier = match self.count {
            0..=1 => 1,
            2..=4 => 2,
            5..=8 => 3,
            9..=13 => 4,
            _ => 5,
        };

        self.timer = self.window;

        let base_score: u64 = if is_warp { 250 } else { 100 };
        let awarded = base_score * (self.multiplier as u64);
        self.score += awarded;
        awarded
    }

    /// Ticks the combo decay timer.
    pub fn update(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer = (self.timer - dt).max(0.0);
            if self.timer == 0.0 {
                // Combo expired
                self.count = 0;
                self.multiplier = 1;
            }
        }
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.multiplier = 1;
        self.timer = 0.0;
        self.score = 0;
        self.highest_combo = 0;
        self.total_kills = 0;
        self.warp_kills = 0;
    }
}
