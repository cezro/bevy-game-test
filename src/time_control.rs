use bevy::prelude::*;

/// Global state controlling time dilation (slow motion), hitstop, and camera shake.
#[derive(Resource, Debug, Clone)]
pub struct ChronoState {
    /// World time dilation scale (e.g. 0.15 for slow motion, 1.0 for normal time).
    pub dilation_factor: f32,
    /// Whether time dilation is actively requested and sustained by the player.
    pub is_dilating: bool,
    /// Hitstop timer: freezes world and player briefly upon landing a successful slash.
    pub hitstop_timer: f32,
    /// Screen shake intensity (magnitude in pixels).
    pub screen_shake: f32,
    /// Current screen shake offset to apply to camera.
    pub shake_offset: Vec2,
    /// Minimum dilation speed factor during slow motion.
    pub slowmo_factor: f32,
}

impl Default for ChronoState {
    fn default() -> Self {
        Self {
            dilation_factor: 1.0,
            is_dilating: false,
            hitstop_timer: 0.0,
            screen_shake: 0.0,
            shake_offset: Vec2::ZERO,
            slowmo_factor: 0.15,
        }
    }
}

impl ChronoState {
    pub const HITSTOP_NORMAL: f32 = 0.045; // 45ms hitstop
    pub const HITSTOP_WARP: f32 = 0.070;   // 70ms hitstop for teleport slash

    /// Updates time control timers, screen shake decay, and dilation factor.
    pub fn update(&mut self, unscaled_dt: f32) {
        // Hitstop countdown
        if self.hitstop_timer > 0.0 {
            self.hitstop_timer = (self.hitstop_timer - unscaled_dt).max(0.0);
        }

        // Screen shake decay
        if self.screen_shake > 0.0 {
            self.screen_shake = (self.screen_shake - unscaled_dt * 30.0).max(0.0);
            if self.screen_shake > 0.001 {
                // Procedural jitter
                let angle = (unscaled_dt * 1234.5 + self.screen_shake * 987.6).sin() * std::f32::consts::TAU;
                self.shake_offset = Vec2::new(angle.cos(), angle.sin()) * self.screen_shake;
            } else {
                self.shake_offset = Vec2::ZERO;
            }
        } else {
            self.shake_offset = Vec2::ZERO;
        }

        // Dilation factor update
        let target_dilation = if self.is_dilating {
            self.slowmo_factor
        } else {
            1.0
        };

        // Smooth or snappy lerp towards target dilation
        self.dilation_factor = self.dilation_factor + (target_dilation - self.dilation_factor) * (unscaled_dt * 20.0).min(1.0);
    }

    /// Triggers a brief hitstop screen freeze.
    pub fn trigger_hitstop(&mut self, duration: f32) {
        self.hitstop_timer = duration.max(self.hitstop_timer);
    }

    /// Triggers screen shake with given intensity in pixels.
    pub fn trigger_shake(&mut self, intensity: f32) {
        self.screen_shake = (self.screen_shake + intensity).min(20.0);
    }

    /// Checks if hitstop is active (all simulation frozen).
    pub fn is_hitstop_active(&self) -> bool {
        self.hitstop_timer > 0.0
    }

    /// Returns the effective delta time for world entities (enemies, particles, enemy attacks).
    pub fn get_world_dt(&self, real_dt: f32) -> f32 {
        if self.is_hitstop_active() {
            0.0
        } else {
            real_dt * self.dilation_factor
        }
    }

    /// Returns the effective delta time for the player (player moves at full speed during slowmo).
    pub fn get_player_dt(&self, real_dt: f32) -> f32 {
        if self.is_hitstop_active() {
            0.0
        } else {
            // Player is slightly boosted or normal speed during slowmo
            let player_speed_mult = if self.is_dilating { 1.1 } else { 1.0 };
            real_dt * player_speed_mult
        }
    }
}

/// System to tick the ChronoState resource every frame.
pub fn update_chrono_system(mut chrono: ResMut<ChronoState>, time: Res<Time<Real>>) {
    chrono.update(time.delta_secs());
}
