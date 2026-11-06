#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use crate::player::{step_horizontal_velocity, step_vertical_velocity, MovementConfig, ChronoEnergy};
    use crate::time_control::ChronoState;
    use crate::combat::{point_to_segment_distance_sq, segment_intersects_circle, katana_arc_intersects, ComboState};
    use crate::enemy::{step_enemy_ai, calculate_wave_enemy_count, EnemyType, EnemyState, WaveManager, WavePhase, WaveAction};
    use crate::ragdoll::calculate_slice_split_velocities;

    #[test]
    fn test_step_horizontal_velocity_accel_decel() {
        let config = MovementConfig {
            accel: 2000.0,
            decel: 3000.0,
            max_speed: 300.0,
            ..default()
        };

        // Accelerate to the right from standstill
        let dt = 0.05;
        let v1 = step_horizontal_velocity(0.0, 1.0, &config, dt);
        assert_eq!(v1, 100.0); // 0 + 2000 * 0.05

        // Continue accelerating until clamped at max_speed
        let v_max = step_horizontal_velocity(250.0, 1.0, &config, dt);
        assert_eq!(v_max, 300.0); // Clamps to max_speed

        // Decelerate when input is released
        let v_stop = step_horizontal_velocity(100.0, 0.0, &config, dt);
        assert_eq!(v_stop, 0.0); // 100 - 3000 * 0.05 = -50 -> clamped to 0.0
    }

    #[test]
    fn test_step_horizontal_velocity_reverse_direction() {
        let config = MovementConfig {
            accel: 2000.0,
            decel: 3000.0,
            max_speed: 300.0,
            ..default()
        };

        // Moving right (+200), suddenly pressing left (-1)
        let dt = 0.05;
        let v = step_horizontal_velocity(200.0, -1.0, &config, dt);
        // target is -300, diff is -500, step is 100 -> v becomes 100
        assert_eq!(v, 100.0);
    }

    #[test]
    fn test_step_vertical_velocity_jump_and_gravity() {
        let config = MovementConfig {
            jump_force: 500.0,
            gravity: 1000.0,
            fall_gravity_mult: 1.5,
            short_hop_mult: 2.0,
            coyote_time_max: 0.1,
            jump_buffer_max: 0.1,
            ..default()
        };

        let dt = 0.02;
        // Grounded, pressing jump
        let (vy, coyote, buffer, did_jump) = step_vertical_velocity(
            0.0, true, true, true, 0.0, 0.0, &config, dt,
        );
        assert!(did_jump);
        assert_eq!(vy, 500.0);
        assert_eq!(coyote, 0.0);
        assert_eq!(buffer, 0.0);

        // Rising with jump held -> standard gravity
        let (vy_rise, _, _, _) = step_vertical_velocity(
            500.0, false, true, false, 0.0, 0.0, &config, dt,
        );
        assert_eq!(vy_rise, 500.0 - 1000.0 * 0.02); // 480.0

        // Rising with jump released early -> short hop gravity (2x gravity)
        let (vy_short, _, _, _) = step_vertical_velocity(
            500.0, false, false, false, 0.0, 0.0, &config, dt,
        );
        assert_eq!(vy_short, 500.0 - 2000.0 * 0.02); // 460.0

        // Falling -> fall gravity (1.5x gravity)
        let (vy_fall, _, _, _) = step_vertical_velocity(
            -50.0, false, false, false, 0.0, 0.0, &config, dt,
        );
        assert_eq!(vy_fall, -50.0 - 1500.0 * 0.02); // -80.0
    }

    #[test]
    fn test_coyote_time_allows_late_jump() {
        let config = MovementConfig {
            jump_force: 500.0,
            coyote_time_max: 0.12,
            jump_buffer_max: 0.12,
            ..default()
        };

        // Just walked off ledge (is_grounded = false), coyote timer was initialized to 0.10s
        let dt = 0.03;
        let (vy, _, _, did_jump) = step_vertical_velocity(
            -10.0, false, true, true, 0.10, 0.0, &config, dt,
        );
        assert!(did_jump);
        assert_eq!(vy, 500.0);

        // If coyote time has expired (0.0s), cannot jump midair
        let (vy2, _, _, did_jump2) = step_vertical_velocity(
            -10.0, false, true, true, 0.0, 0.0, &config, dt,
        );
        assert!(!did_jump2);
        assert!(vy2 < -10.0);
    }

    #[test]
    fn test_jump_buffer_triggers_on_landing() {
        let config = MovementConfig {
            jump_force: 500.0,
            coyote_time_max: 0.12,
            jump_buffer_max: 0.12,
            ..default()
        };

        // Midair (coyote = 0), player presses jump (buffer = 0.12s)
        let dt = 0.04;
        let (_, _, buffer, did_jump) = step_vertical_velocity(
            -200.0, false, true, true, 0.0, 0.0, &config, dt,
        );
        assert!(!did_jump);
        assert!(buffer > 0.0);

        // Next frame: player lands on ground (is_grounded = true, buffer still active)
        let (vy_land, _, _, did_jump_land) = step_vertical_velocity(
            0.0, true, true, false, 0.0, buffer, &config, dt,
        );
        assert!(did_jump_land);
        assert_eq!(vy_land, 500.0);
    }

    #[test]
    fn test_chrono_energy_drain_and_regen() {
        let mut energy = ChronoEnergy {
            current: 100.0,
            max: 100.0,
            drain_rate: 20.0,
            regen_rate: 10.0,
            teleport_cost: 25.0,
            kill_restore: 20.0,
        };

        // Drain during time dilation
        energy.update(true, 1.0);
        assert_eq!(energy.current, 80.0);

        // Regenerate during normal time
        energy.update(false, 1.0);
        assert_eq!(energy.current, 90.0);

        // Regenerate clamp at max
        energy.update(false, 2.0);
        assert_eq!(energy.current, 100.0);
    }

    #[test]
    fn test_chrono_energy_teleport_cost_and_kill_reward() {
        let mut energy = ChronoEnergy {
            current: 30.0,
            max: 100.0,
            drain_rate: 20.0,
            regen_rate: 10.0,
            teleport_cost: 25.0,
            kill_restore: 20.0,
        };

        assert!(energy.can_teleport());
        let success = energy.consume_teleport();
        assert!(success);
        assert_eq!(energy.current, 5.0);

        // Cannot afford another teleport
        assert!(!energy.can_teleport());
        assert!(!energy.consume_teleport());
        assert_eq!(energy.current, 5.0);

        // Kill reward restores energy
        energy.reward_kill();
        assert_eq!(energy.current, 25.0);
        assert!(energy.can_teleport());
    }

    #[test]
    fn test_chrono_time_dilation_math() {
        let mut chrono = ChronoState {
            dilation_factor: 1.0,
            is_dilating: true,
            hitstop_timer: 0.0,
            slowmo_factor: 0.15,
            ..default()
        };

        chrono.update(0.1); // Lerp towards slowmo_factor
        assert!(chrono.dilation_factor < 0.5);

        // Directly set to 0.15 for exact check
        chrono.dilation_factor = 0.15;
        let real_dt = 0.1;
        let world_dt = chrono.get_world_dt(real_dt);
        let player_dt = chrono.get_player_dt(real_dt);

        assert!((world_dt - 0.015).abs() < 1e-5);
        assert!(player_dt >= real_dt); // Player is not slowed down!

        // Hitstop freezes both world and player
        chrono.trigger_hitstop(0.05);
        assert_eq!(chrono.get_world_dt(real_dt), 0.0);
        assert_eq!(chrono.get_player_dt(real_dt), 0.0);
    }

    #[test]
    fn test_point_to_line_segment_distance_and_circle_intersection() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(100.0, 0.0);

        // Point exactly on the segment
        let dist_sq = point_to_segment_distance_sq(Vec2::new(50.0, 0.0), a, b);
        assert_eq!(dist_sq, 0.0);

        // Point 10 units above segment midpoint
        let dist_sq_above = point_to_segment_distance_sq(Vec2::new(50.0, 10.0), a, b);
        assert_eq!(dist_sq_above, 100.0);

        // Point past endpoint B
        let dist_sq_past = point_to_segment_distance_sq(Vec2::new(110.0, 0.0), a, b);
        assert_eq!(dist_sq_past, 100.0);

        // Circle intersection
        let enemy_center = Vec2::new(50.0, 15.0);
        assert!(segment_intersects_circle(a, b, enemy_center, 18.0));
        assert!(!segment_intersects_circle(a, b, enemy_center, 12.0));
    }

    #[test]
    fn test_katana_arc_intersection() {
        let origin = Vec2::new(0.0, 0.0);
        let facing = 1.0;
        let aim_dir = Vec2::new(1.0, 0.0);
        let slash_range = 80.0;
        let arc_rad = std::f32::consts::PI * 0.75; // ~135 degrees

        // Target directly in front within range
        let target_in_front = Vec2::new(50.0, 10.0);
        assert!(katana_arc_intersects(origin, facing, aim_dir, target_in_front, slash_range, arc_rad, 15.0));

        // Target behind player
        let target_behind = Vec2::new(-50.0, 0.0);
        assert!(!katana_arc_intersects(origin, facing, aim_dir, target_behind, slash_range, arc_rad, 15.0));

        // Target too far away
        let target_far = Vec2::new(120.0, 0.0);
        assert!(!katana_arc_intersects(origin, facing, aim_dir, target_far, slash_range, arc_rad, 15.0));
    }

    #[test]
    fn test_combo_multiplier_scaling_and_decay() {
        let mut combo = ComboState::default();
        assert_eq!(combo.multiplier, 1);

        // Kill 1
        combo.register_kill(false);
        assert_eq!(combo.count, 1);
        assert_eq!(combo.multiplier, 1);
        assert_eq!(combo.score, 100);

        // Kill 2 -> 2x multiplier
        combo.register_kill(false);
        assert_eq!(combo.count, 2);
        assert_eq!(combo.multiplier, 2);
        assert_eq!(combo.score, 300); // 100 + 100*2

        // Warp kills give 250 base score
        combo.register_kill(true);
        assert_eq!(combo.count, 3);
        assert_eq!(combo.multiplier, 2);
        assert_eq!(combo.score, 800); // 300 + 250*2

        // Decay timer expiration resets combo
        combo.update(3.0);
        assert_eq!(combo.count, 0);
        assert_eq!(combo.multiplier, 1);
        // Score remains preserved
        assert_eq!(combo.score, 800);
    }

    #[test]
    fn test_enemy_ai_windup_transition_and_jump() {
        let enemy_pos = Vec2::new(40.0, 0.0);
        let player_pos = Vec2::new(0.0, 0.0);
        let state = EnemyState::Chase;

        // In close range (<52.0), should enter Windup
        let (next_state, _, _) = step_enemy_ai(
            enemy_pos, player_pos, &state, EnemyType::Runner, 52.0, true, 0.016,
        );
        match next_state {
            EnemyState::Windup { .. } => {}
            _ => panic!("Expected enemy to transition to Windup"),
        }

        // Higher player elevation triggers jump
        let enemy_pos_low = Vec2::new(80.0, -100.0);
        let player_pos_high = Vec2::new(40.0, 0.0);
        let (_, _, should_jump) = step_enemy_ai(
            enemy_pos_low, player_pos_high, &state, EnemyType::Runner, 52.0, true, 0.016,
        );
        assert!(should_jump);
    }

    #[test]
    fn test_wave_scaling_formula() {
        assert_eq!(calculate_wave_enemy_count(1), 6);
        assert_eq!(calculate_wave_enemy_count(2), 9);
        assert_eq!(calculate_wave_enemy_count(3), 12);
        assert_eq!(calculate_wave_enemy_count(5), 18);
    }

    #[test]
    fn test_wave_manager_progression() {
        let mut mgr = WaveManager {
            current_wave: 1,
            phase: WavePhase::Intermission { timer: 0.1 },
            total_wave_enemies: 6,
            alive_count: 0,
            spawn_interval: 1.0,
            intermission_duration: 2.0,
        };

        // Intermission countdown finishes -> starts spawning
        let action = mgr.update(0.15, 1000.0, -200.0);
        match action {
            Some(WaveAction::AnnounceWave(1)) => {}
            _ => panic!("Expected AnnounceWave action"),
        }

        // Spawn first enemy
        let spawn_action = mgr.update(0.15, 1000.0, -200.0);
        match spawn_action {
            Some(WaveAction::SpawnEnemy(..)) => {}
            _ => panic!("Expected SpawnEnemy action"),
        }
        assert_eq!(mgr.alive_count, 1);
    }

    #[test]
    fn test_ragdoll_slice_velocities_split() {
        let base_vel = Vec2::new(100.0, 0.0);
        let slice_dir = Vec2::new(1.0, 0.0);
        let impulse = Vec2::new(200.0, 50.0);

        let (upper_vel, upper_rot, lower_vel, lower_rot) = calculate_slice_split_velocities(base_vel, slice_dir, impulse);

        // Upper half gets positive upward kick and normal
        assert!(upper_vel.y > 0.0);
        // Angular velocities rotate in opposite or divergent directions
        assert_ne!(upper_rot, lower_rot);
        // Both halves have received distinct separating velocities
        assert!((upper_vel - lower_vel).length() > 50.0);
    }

    #[test]
    fn test_degenerate_zero_length_teleport_segment() {
        let a = Vec2::new(50.0, 50.0);
        let b = Vec2::new(50.0, 50.0); // Identical start and end point
        let target = Vec2::new(50.0, 60.0);

        let dist_sq = point_to_segment_distance_sq(target, a, b);
        assert_eq!(dist_sq, 100.0); // 10 units away

        assert!(segment_intersects_circle(a, b, target, 11.0));
        assert!(!segment_intersects_circle(a, b, target, 9.0));
    }

    #[test]
    fn test_zero_slice_direction_ragdoll_velocity_no_nan() {
        let base_vel = Vec2::ZERO;
        let zero_slice_dir = Vec2::ZERO; // Degenerate zero vector
        let impulse = Vec2::ZERO;

        let (upper_vel, upper_rot, lower_vel, lower_rot) =
            calculate_slice_split_velocities(base_vel, zero_slice_dir, impulse);

        assert!(!upper_vel.is_nan());
        assert!(!lower_vel.is_nan());
        assert!(!upper_rot.is_nan());
        assert!(!lower_rot.is_nan());
    }

    #[test]
    fn test_hitstop_duration_max_stacking() {
        let mut chrono = ChronoState::default();
        chrono.trigger_hitstop(0.04);
        assert_eq!(chrono.hitstop_timer, 0.04);

        // Triggering a shorter hitstop does not diminish existing longer hitstop
        chrono.trigger_hitstop(0.02);
        assert_eq!(chrono.hitstop_timer, 0.04);

        // Triggering a longer hitstop extends it
        chrono.trigger_hitstop(0.07);
        assert_eq!(chrono.hitstop_timer, 0.07);
    }

    #[test]
    fn test_screen_shake_clamping_and_decay() {
        let mut chrono = ChronoState::default();
        chrono.trigger_shake(35.0);
        assert_eq!(chrono.screen_shake, 20.0); // Clamped at 20.0

        chrono.update(0.5); // Decay
        assert!(chrono.screen_shake < 20.0);
    }

    #[test]
    fn test_combo_multiplier_all_tier_boundaries() {
        let mut combo = ComboState::default();
        // Tier 1: count 0..=1 -> 1x
        combo.register_kill(false); // count = 1
        assert_eq!(combo.multiplier, 1);

        // Tier 2: count 2..=4 -> 2x
        combo.register_kill(false); // count = 2
        assert_eq!(combo.multiplier, 2);
        combo.register_kill(false); // count = 3
        assert_eq!(combo.multiplier, 2);
        combo.register_kill(false); // count = 4
        assert_eq!(combo.multiplier, 2);

        // Tier 3: count 5..=8 -> 3x
        combo.register_kill(false); // count = 5
        assert_eq!(combo.multiplier, 3);
        combo.register_kill(false); // count = 6
        combo.register_kill(false); // count = 7
        combo.register_kill(false); // count = 8
        assert_eq!(combo.multiplier, 3);

        // Tier 4: count 9..=13 -> 4x
        combo.register_kill(false); // count = 9
        assert_eq!(combo.multiplier, 4);
        for _ in 10..=13 {
            combo.register_kill(false);
        }
        assert_eq!(combo.multiplier, 4);

        // Tier 5: count >= 14 -> 5x
        combo.register_kill(false); // count = 14
        assert_eq!(combo.multiplier, 5);
        combo.register_kill(false); // count = 15
        assert_eq!(combo.multiplier, 5);
    }

            #[test]
    fn test_adversarial_ragdoll_piece_leak_verification() {
        use crate::ragdoll::{update_ragdoll_physics, RagdollPiece, RagdollPart};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(100));
        });
        app.add_systems(Update, update_ragdoll_physics);

        let entity = app.world_mut().spawn((
            RagdollPiece::new(
                RagdollPart::UpperTorso,
                Vec2::ZERO,
                0.0,
                1.0,
                Color::WHITE,
            ),
            Transform::from_xyz(0.0, 0.0, 0.0),
        )).id();

        // Advance 6.0 seconds (lifetime is 4.5s)
        for _ in 0..60 {
            {
                let mut time = app.world_mut().resource_mut::<Time<Real>>();
                time.advance_by(std::time::Duration::from_millis(100));
            }
            app.update();
        }

        // The entity MUST be despawned once expired to prevent an unbounded memory leak!
        let exists = app.world().get_entity(entity).is_ok();
        assert!(
            !exists,
            "CRITICAL FLAW: RagdollPiece entity was NOT despawned after lifetime expired! (Entity leak detected)"
        );
    }

    #[test]
    fn test_adversarial_duplicate_slice_events_double_decrement_alive_count() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<ComboState>();
        app.init_resource::<WaveManager>();
        app.add_event::<SliceEnemyEvent>();
        app.add_systems(Update, slice_event_resolution_system);

        let enemy_entity = app.world_mut().spawn((
            Enemy::new(EnemyType::Runner),
            Transform::from_xyz(0.0, 0.0, 0.0),
        )).id();

        // Wave manager has 1 enemy alive
        app.world_mut().resource_mut::<WaveManager>().alive_count = 1;

        // Send TWO slice events for the exact same enemy in the same frame
        {
            let mut events = app.world_mut().resource_mut::<Events<SliceEnemyEvent>>();
            events.send(SliceEnemyEvent {
                enemy_entity,
                slice_position: Vec2::ZERO,
                slice_direction: Vec2::X,
                impulse: Vec2::ZERO,
                is_warp_kill: false,
            });
            events.send(SliceEnemyEvent {
                enemy_entity,
                slice_position: Vec2::ZERO,
                slice_direction: Vec2::X,
                impulse: Vec2::ZERO,
                is_warp_kill: true,
            });
        }

        // Run system update
        app.update();

        let alive = app.world().resource::<WaveManager>().alive_count;
        let combo_kills = app.world().resource::<ComboState>().total_kills;

        assert_eq!(
            combo_kills, 1,
            "CRITICAL FLAW: Single enemy registered {} kills due to un-deduplicated slice events!", combo_kills
        );
        assert_eq!(
            alive, 0,
            "CRITICAL FLAW: Alive count decremented multiple times for a single entity!"
        );
    }

    #[test]
    fn test_adversarial_wave_phase_zero_enemies_deadlock() {
        let mut mgr = WaveManager {
            current_wave: 0,
            phase: WavePhase::Spawning {
                remaining_to_spawn: 0,
                spawn_timer: 0.1,
            },
            total_wave_enemies: 0,
            alive_count: 0,
            spawn_interval: 1.0,
            intermission_duration: 2.0,
        };

        // Update several times
        for _ in 0..10 {
            mgr.update(0.5, 1000.0, -200.0);
        }

        // If remaining_to_spawn is 0, it should transition out of Spawning!
        match mgr.phase {
            WavePhase::Spawning { .. } => panic!("CRITICAL FLAW: WaveManager deadlocked in Spawning phase with 0 remaining enemies!"),
            _ => {}
        }
    }

        #[test]
    fn test_adversarial_enemy_out_of_bounds_recovery() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(50));
        });
        app.add_systems(Update, enemy_ai_system);

        // Spawn a dummy player at center
        app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(0.0, -260.0, 0.0),
        ));

        // Spawn an enemy way below the arena floor (out of bounds at y = -500)
        let enemy_entity = app.world_mut().spawn((
            Enemy::new(EnemyType::Runner),
            Transform::from_xyz(100.0, -500.0, 0.0),
        )).id();

        // Run update
        app.update();

        // Enemy transform must be recovered to floor_y (-260.0)
        let transform = app.world().get::<Transform>(enemy_entity).unwrap();
        assert_eq!(
            transform.translation.y, -260.0,
            "Enemy falling out-of-bounds must be safely recovered to floor_y!"
        );
    }

    #[test]
    fn test_burnout_multiplier_mastered_previous_tier_vs_skip() {
        use crate::chrono_shift::{Gear, GearAcclimatization};

        let mut accl = GearAcclimatization::default();
        // At tier 0: Neutral is mastered, so Gear 1 has ZERO burnout multiplier (1.0x tolerable drain)
        assert_eq!(accl.burnout_multiplier(Gear::G1), 1.0);

        // Skipping ahead to Gear 2 when only Neutral is mastered:
        // Formula: 2.5 + (2 - 1 - 0)*2.0 + (1.0 - 0.0)*2.0 = 6.5
        assert_eq!(accl.burnout_multiplier(Gear::G2), 6.5);
        // Skipping to Gear 3:
        assert_eq!(accl.burnout_multiplier(Gear::G3), 8.5);
        // Skipping to Gear 6:
        assert_eq!(accl.burnout_multiplier(Gear::G6), 14.5);

        // Master Gear 1
        accl.on_flawless_kill(Gear::G1, 2);
        assert_eq!(accl.mastered_tier, 1);

        // Now Gear 2 has previous gear (G1) mastered -> returns 1.0 (ZERO burnout!)
        assert_eq!(accl.burnout_multiplier(Gear::G2), 1.0);
        // Gear 1 itself is also mastered -> returns 1.0
        assert_eq!(accl.burnout_multiplier(Gear::G1), 1.0);
        // Gear 3 is still skipped (1 < 3 - 1) -> 2.5 + 2.0 + 2.0 = 6.5
        assert_eq!(accl.burnout_multiplier(Gear::G3), 6.5);
    }

    #[test]
    fn test_gunner_ai_tactical_distance_and_shooting() {
        // 1. Semi-auto gunner backpedals if player < 140px
        let gunner_pos = Vec2::new(100.0, 0.0);
        let player_pos = Vec2::new(30.0, 0.0); // dist_x = 70 (< 140)
        let (state, move_dir, _) = step_enemy_ai(
            gunner_pos,
            player_pos,
            &EnemyState::Chase,
            EnemyType::GunnerSemi,
            360.0,
            true,
            0.016,
        );
        assert_eq!(state, EnemyState::Chase);
        assert_eq!(move_dir, 1.0, "Gunner facing left must backpedal to the right (+1.0)!");

        // 2. Gunner advances if player > attack_range (360px)
        let player_far = Vec2::new(500.0, 0.0); // dist_x = 400 (> 360)
        let (state_far, move_dir_far, _) = step_enemy_ai(
            gunner_pos,
            player_far,
            &EnemyState::Chase,
            EnemyType::GunnerSemi,
            360.0,
            true,
            0.016,
        );
        assert_eq!(state_far, EnemyState::Chase);
        assert_eq!(move_dir_far, 1.0, "Gunner must advance towards player (+1.0)!");

        // 3. Gunner enters Windup when within tactical distance [140, 360]
        let player_sweet_spot = Vec2::new(300.0, 0.0); // dist_x = 200
        let (state_aim, _, _) = step_enemy_ai(
            gunner_pos,
            player_sweet_spot,
            &EnemyState::Chase,
            EnemyType::GunnerSemi,
            360.0,
            true,
            0.016,
        );
        match state_aim {
            EnemyState::Windup { duration, .. } => {
                assert!((duration - 0.45).abs() < 1e-4);
            }
            _ => panic!("Expected GunnerSemi to enter Windup state when in range!"),
        }

        // 4. Windup transitions to Attack
        let (state_atk, _, _) = step_enemy_ai(
            gunner_pos,
            player_sweet_spot,
            &EnemyState::Windup { timer: 0.01, duration: 0.45 },
            EnemyType::GunnerSemi,
            360.0,
            true,
            0.02,
        );
        match state_atk {
            EnemyState::Attack { .. } => {}
            _ => panic!("Expected Windup to transition to Attack!"),
        }
    }

    #[test]
    fn test_gunner_spawning_and_bullet_firing_system() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(50));
        });
        app.add_systems(Update, enemy_ai_system);

        // Spawn player
        app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(0.0, -260.0, 0.0),
        ));

        // Spawn GunnerSemi in Attack state
        app.world_mut().spawn((
            Enemy {
                enemy_type: EnemyType::GunnerSemi,
                state: EnemyState::Attack { timer: 0.35, duration: 0.35, has_hit: false },
                velocity: Vec2::ZERO,
                facing: -1.0,
                is_grounded: true,
                health: 1.0,
                max_health: 1.0,
                attack_range: 360.0,
                speed: 160.0,
                jump_force: 460.0,
                hitbox_radius: 18.0,
                attack_damage: 20.0,
                bullets_to_fire: 0,
                fire_cooldown: 0.0,
            },
            Transform::from_xyz(200.0, -260.0, 0.0),
        ));

        // Run update to execute bullet firing
        app.update();

        // Query spawned bullet
        let mut bullet_query = app.world_mut().query::<(&Transform, &Bullet)>();
        let bullet_count = bullet_query.iter(app.world()).count();
        assert_eq!(bullet_count, 1, "GunnerSemi must fire exactly 1 bullet per attack!");

        let (b_transform, bullet) = bullet_query.iter(app.world()).next().unwrap();
        assert!(!bullet.is_automatic);
        assert_eq!(bullet.damage, 20.0);
        // Bullet should fly towards player at x = 0 (negative velocity x)
        assert!(bullet.velocity.x < 0.0);
        assert_eq!(b_transform.translation.z, 6.0);
    }

    #[test]
    fn test_bullet_system_slow_motion_dilation() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(100)); // 0.1s
        });
        app.add_systems(Update, bullet_system);

        // Spawn player far away
        app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(-500.0, -260.0, 0.0),
        ));

        // Spawn bullet with velocity 500 px/s in normal time (slowmo = 1.0)
        let bullet_entity = app.world_mut().spawn((
            Bullet {
                velocity: Vec2::new(500.0, 0.0),
                radius: 6.0,
                damage: 20.0,
                lifetime: 3.0,
                is_automatic: false,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        )).id();

        app.update();

        let pos1 = app.world().get::<Transform>(bullet_entity).unwrap().translation.x;
        // Moved 500.0 * 0.1s = 50.0 px
        assert!((pos1 - 50.0).abs() < 1.0);

        // Now activate Gear 6 slow motion (dilation = 0.04)
        {
            let mut chrono = app.world_mut().resource_mut::<ChronoState>();
            chrono.dilation_factor = 0.04;
            chrono.slowmo_factor = 0.04;
            chrono.is_dilating = true;
        }

        app.update();

        let pos2 = app.world().get::<Transform>(bullet_entity).unwrap().translation.x;
        let delta = pos2 - pos1;
        // In Gear 6 slow motion: 500.0 * 0.1s * 0.04 = 2.0 px!
        assert!((delta - 2.0).abs() < 0.1, "Bullet must crawl at 2.0px/frame in slow-mo, got {}", delta);
    }

    #[test]
    fn test_katana_slash_slashes_bullet_and_reduces_cooldown() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<CursorWorldCoords>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.add_event::<SliceEnemyEvent>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(Update, player_katana_slash_system);

        // Cursor pointing forward (right)
        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(100.0, 0.0));

        // Left mouse click pressed
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);

        // Spawn player with zero slash cooldown
        let player_entity = app.world_mut().spawn((
            Player {
                slash_cooldown: 0.0,
                facing: 1.0,
                ..default()
            },
            Transform::from_xyz(0.0, -16.0, 0.0), // center at (0, 0)
        )).id();

        // Spawn a bullet directly in front of the player (within 80px katana arc)
        let bullet_entity = app.world_mut().spawn((
            Bullet {
                velocity: Vec2::new(-400.0, 0.0),
                radius: 6.0,
                damage: 20.0,
                lifetime: 2.0,
                is_automatic: false,
            },
            Transform::from_xyz(40.0, 0.0, 0.0),
        )).id();

        app.update();

        // 1. Bullet must be destroyed / despawned
        assert!(app.world().get_entity(bullet_entity).is_err(), "Bullet must be destroyed by katana slash!");

        // 2. Slash cooldown must be reduced to 0.04 (rapid chaining!)
        let player = app.world().get::<Player>(player_entity).unwrap();
        assert_eq!(player.slash_cooldown, 0.04, "Slash cooldown must be reduced to 0.04 when slashing a bullet!");
    }

    #[test]
    fn test_katana_slash_hits_enemy_and_reduces_cooldown() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<CursorWorldCoords>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.add_event::<SliceEnemyEvent>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(Update, player_katana_slash_system);

        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(100.0, 0.0));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);

        let player_entity = app.world_mut().spawn((
            Player {
                slash_cooldown: 0.0,
                facing: 1.0,
                ..default()
            },
            Transform::from_xyz(0.0, -16.0, 0.0),
        )).id();

        // Spawn enemy in katana arc
        app.world_mut().spawn((
            Enemy::new(EnemyType::Runner),
            Transform::from_xyz(45.0, -16.0, 0.0),
        ));

        app.update();

        let player = app.world().get::<Player>(player_entity).unwrap();
        assert_eq!(player.slash_cooldown, 0.04, "Slash cooldown must be reduced to 0.04 when slashing an enemy!");
    }

    #[test]
    fn test_chrono_warp_hits_enemy_reduces_cooldown_but_not_bullets() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<CursorWorldCoords>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_event::<SliceEnemyEvent>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(16));
        });
        app.add_systems(Update, player_chrono_warp_system);

        // Warp Key Q pressed towards (200, -200)
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyQ);
        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(200.0, -200.0));

        let player_entity = app.world_mut().spawn((
            Player {
                warp_cooldown: 0.0,
                ..default()
            },
            ChronoEnergy::default(),
            Transform::from_xyz(0.0, -200.0, 0.0),
        )).id();

        // Spawn enemy right along warp path
        let enemy_entity = app.world_mut().spawn((
            Enemy::new(EnemyType::Runner),
            Transform::from_xyz(100.0, -200.0, 0.0),
        )).id();

        app.update();

        let player = app.world().get::<Player>(player_entity).unwrap();
        assert_eq!(player.warp_cooldown, 0.05, "Warp cooldown must be reduced to 0.05 when warping through an enemy!");

        // Despawn sliced enemy so it is no longer on the path
        app.world_mut().despawn(enemy_entity);

        // Reset warp cooldown to 0 and warp through ONLY a bullet (no enemies)
        {
            let mut p = app.world_mut().get_mut::<Player>(player_entity).unwrap();
            p.warp_cooldown = 0.0;
        }
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyQ);
        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(0.0, -200.0));

        // Spawn only a bullet along the path
        app.world_mut().spawn((
            Bullet {
                velocity: Vec2::ZERO,
                radius: 10.0,
                damage: 10.0,
                lifetime: 2.0,
                is_automatic: false,
            },
            Transform::from_xyz(100.0, -200.0, 0.0),
        ));

        app.update();

        let player2 = app.world().get::<Player>(player_entity).unwrap();
        // Since no enemy was hit, warp cooldown is NOT reduced and stays at default 0.20!
        assert_eq!(player2.warp_cooldown, 0.20, "Warp cooldown must NOT be reduced on bullets (enemies only)!");
    }

    #[test]
    fn test_wave_cleared_real_time_transition() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<WaveManager>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(500)); // 0.5s real time
        });
        app.add_systems(Update, wave_manager_system);

        // Put wave manager in WaveCleared phase with 2.0s countdown
        app.world_mut().resource_mut::<WaveManager>().phase = WavePhase::WaveCleared { timer: 2.0 };

        // Turn on extreme Gear 6 slow motion dilation (0.04x)
        {
            let mut chrono = app.world_mut().resource_mut::<ChronoState>();
            chrono.slowmo_factor = 0.04;
            chrono.is_dilating = true;
        }

        // Run update: real time 0.5s passes
        app.update();

        // In real time, the countdown must advance by 0.5s (remaining = 1.5s), NOT 0.5 * 0.04 = 0.02s!
        let phase = &app.world().resource::<WaveManager>().phase;
        match phase {
            WavePhase::WaveCleared { timer } => {
                assert!((timer - 1.5).abs() < 1e-3, "WaveCleared must advance in real time! Got {}", timer);
            }
            _ => panic!("Expected WaveCleared phase"),
        }

        // Test Intermission phase also advances in real time
        app.world_mut().resource_mut::<WaveManager>().phase = WavePhase::Intermission { timer: 2.5 };
        app.update();
        let phase2 = &app.world().resource::<WaveManager>().phase;
        match phase2 {
            WavePhase::Intermission { timer } => {
                assert!((timer - 2.0).abs() < 1e-3, "Intermission must advance in real time! Got {}", timer);
            }
            _ => panic!("Expected Intermission phase"),
        }
    }

    #[test]
    fn test_wave_manager_spawn_distribution_gunners() {
        let mut mgr = WaveManager {
            current_wave: 2,
            phase: WavePhase::Spawning {
                remaining_to_spawn: 6,
                spawn_timer: 0.0,
            },
            total_wave_enemies: 6,
            alive_count: 0,
            spawn_interval: 1.0,
            intermission_duration: 2.0,
        };

        // In Wave 2: collect all spawned enemy types across the wave
        let mut wave2_types = Vec::new();
        while mgr.phase != WavePhase::Active {
            if let WavePhase::Spawning { remaining_to_spawn, .. } = mgr.phase {
                mgr.phase = WavePhase::Spawning { remaining_to_spawn, spawn_timer: 0.0 };
            }
            if let Some(WaveAction::SpawnEnemy(e_type, _)) = mgr.update(0.01, 1000.0, -200.0) {
                wave2_types.push(e_type);
            }
        }
        assert!(wave2_types.contains(&EnemyType::GunnerSemi), "Wave 2 must spawn GunnerSemi!");
        assert!(wave2_types.contains(&EnemyType::Ninja), "Wave 2 must spawn Ninja!");
        assert!(wave2_types.contains(&EnemyType::Runner), "Wave 2 must spawn Runner!");

        // In Wave 3+: collect all spawned enemy types
        let mut mgr3 = WaveManager {
            current_wave: 3,
            phase: WavePhase::Spawning {
                remaining_to_spawn: 12,
                spawn_timer: 0.0,
            },
            total_wave_enemies: 12,
            alive_count: 0,
            spawn_interval: 1.0,
            intermission_duration: 2.0,
        };
        let mut wave3_types = Vec::new();
        while mgr3.phase != WavePhase::Active {
            if let WavePhase::Spawning { remaining_to_spawn, .. } = mgr3.phase {
                mgr3.phase = WavePhase::Spawning { remaining_to_spawn, spawn_timer: 0.0 };
            }
            if let Some(WaveAction::SpawnEnemy(e_type, _)) = mgr3.update(0.01, 1000.0, -200.0) {
                wave3_types.push(e_type);
            }
        }
        assert!(wave3_types.contains(&EnemyType::GunnerAuto), "Wave 3 must spawn GunnerAuto!");
        assert!(wave3_types.contains(&EnemyType::GunnerSemi), "Wave 3 must spawn GunnerSemi!");
        assert!(wave3_types.contains(&EnemyType::Heavy), "Wave 3 must spawn Heavy!");
    }

    #[test]
    fn test_adversarial_bullet_slice_race_condition() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<CursorWorldCoords>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.add_event::<SliceEnemyEvent>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(16));
        });
        // Both systems running in the same Update cycle with chained ordering (matching main.rs)
        app.add_systems(Update, (player_katana_slash_system, bullet_system).chain());

        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(10.0, 0.0));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);

        let player_entity = app.world_mut().spawn((
            Player {
                health: 100.0,
                slash_cooldown: 0.0,
                invulnerability_timer: 0.0,
                facing: 1.0,
                ..default()
            },
            Transform::from_xyz(0.0, -16.0, 0.0),
        )).id();

        // Spawn incoming bullet 10px in front of player (inside collision radius of 22px)
        app.world_mut().spawn((
            Bullet {
                velocity: Vec2::new(-300.0, 0.0),
                radius: 6.0,
                damage: 20.0,
                lifetime: 3.0,
                is_automatic: false,
            },
            Transform::from_xyz(10.0, 0.0, 0.0),
        ));

        app.update();

        let player = app.world().get::<Player>(player_entity).unwrap();
        assert_eq!(
            player.health,
            100.0,
            "Player sliced incoming bullet! Player health must NOT be reduced (was damaged by race condition)!"
        );
    }

    #[test]
    fn test_adversarial_gunner_multi_attack_cycle_permanent_lockout() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(50));
        });
        app.add_systems(Update, enemy_ai_system);

        let _player_entity = app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(0.0, -260.0, 0.0),
        )).id();

        // Spawn GunnerSemi
        let gunner_entity = app.world_mut().spawn((
            Enemy {
                enemy_type: EnemyType::GunnerSemi,
                state: EnemyState::Attack { timer: 0.35, duration: 0.35, has_hit: false },
                velocity: Vec2::ZERO,
                facing: -1.0,
                is_grounded: true,
                health: 1.0,
                max_health: 1.0,
                attack_range: 360.0,
                speed: 160.0,
                jump_force: 460.0,
                hitbox_radius: 18.0,
                attack_damage: 20.0,
                bullets_to_fire: 0,
                fire_cooldown: 0.0,
            },
            Transform::from_xyz(200.0, -260.0, 0.0),
        )).id();

        // 1. Attack cycle 1: fires bullet
        app.update();
        let bullet_count_1 = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert_eq!(bullet_count_1, 1, "Gunner must fire 1 bullet in attack 1!");

        // Despawn bullet from attack 1 so we can count attack 2 bullets cleanly
        let old_bullets: Vec<Entity> = app.world_mut().query::<(Entity, &Bullet)>().iter(app.world()).map(|(e, _)| e).collect();
        for b_e in old_bullets {
            app.world_mut().despawn(b_e);
        }

        // Simulate enemy finishing attack 1 and entering attack 2
        {
            let mut gunner = app.world_mut().get_mut::<Enemy>(gunner_entity).unwrap();
            gunner.state = EnemyState::Attack { timer: 0.35, duration: 0.35, has_hit: false };
        }

        // 2. Attack cycle 2: gunner must NOT be locked out and must fire bullet in attack 2!
        app.update();
        let bullet_count_2 = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert_eq!(
            bullet_count_2,
            1,
            "GunnerSemi must fire a bullet in attack 2! (Permanent lockout occurred if 0)"
        );
    }

    #[test]
    fn test_adversarial_gunner_auto_burst_and_subsequent_cycle() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(30)); // 30ms per tick
        });
        app.add_systems(Update, enemy_ai_system);

        let _player = app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(0.0, -260.0, 0.0),
        )).id();

        // Spawn GunnerAuto in Attack state
        let gunner = app.world_mut().spawn((
            Enemy {
                enemy_type: EnemyType::GunnerAuto,
                state: EnemyState::Attack { timer: 0.45, duration: 0.45, has_hit: false },
                velocity: Vec2::ZERO,
                facing: -1.0,
                is_grounded: true,
                health: 1.0,
                max_health: 1.0,
                attack_range: 280.0,
                speed: 180.0,
                jump_force: 480.0,
                hitbox_radius: 18.0,
                attack_damage: 10.0,
                bullets_to_fire: 0,
                fire_cooldown: 0.0,
            },
            Transform::from_xyz(200.0, -260.0, 0.0),
        )).id();

        // Tick across the 0.45s attack duration (15 ticks * 0.03s = 0.45s)
        for _ in 0..15 {
            app.update();
        }

        let total_bullets = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert_eq!(total_bullets, 4, "GunnerAuto must fire all 4 bullets during burst attack! Got {}", total_bullets);

        // Despawn old bullets
        let old_bullets: Vec<Entity> = app.world_mut().query::<(Entity, &Bullet)>().iter(app.world()).map(|(e, _)| e).collect();
        for b in old_bullets {
            app.world_mut().despawn(b);
        }

        // Trigger attack 2
        {
            let mut g = app.world_mut().get_mut::<Enemy>(gunner).unwrap();
            g.state = EnemyState::Attack { timer: 0.45, duration: 0.45, has_hit: false };
        }

        // Tick across attack 2
        for _ in 0..15 {
            app.update();
        }

        let attack2_bullets = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert_eq!(attack2_bullets, 4, "GunnerAuto must be capable of firing all 4 bullets in subsequent attacks!");
    }

    #[test]
    fn test_adversarial_slash_both_enemy_and_bullet_simultaneously() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.init_resource::<CursorWorldCoords>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<ComboState>();
        app.init_resource::<WaveManager>();
        app.add_event::<SliceEnemyEvent>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(16));
        });
        app.add_systems(
            Update,
            (
                player_katana_slash_system,
                bullet_system,
                slice_event_resolution_system,
            ).chain(),
        );

        app.world_mut().resource_mut::<CursorWorldCoords>().0 = Some(Vec2::new(50.0, 0.0));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);

        let player = app.world_mut().spawn((
            Player {
                health: 100.0,
                slash_cooldown: 0.0,
                invulnerability_timer: 0.0,
                facing: 1.0,
                ..default()
            },
            ChronoEnergy::default(),
            Transform::from_xyz(0.0, -16.0, 0.0),
        )).id();

        // Spawn enemy in slash arc
        let enemy = app.world_mut().spawn((
            Enemy::new(EnemyType::Runner),
            Transform::from_xyz(40.0, -16.0, 0.0),
        )).id();

        // Spawn bullet in slash arc (at 20px, inside collision radius)
        let bullet = app.world_mut().spawn((
            Bullet {
                velocity: Vec2::new(-300.0, 0.0),
                radius: 6.0,
                damage: 20.0,
                lifetime: 2.0,
                is_automatic: false,
            },
            Transform::from_xyz(20.0, 0.0, 0.0),
        )).id();

        app.update();

        // Check outcomes:
        let p = app.world().get::<Player>(player).unwrap();
        // Cooldown properly reduced to 0.04
        assert_eq!(p.slash_cooldown, 0.04, "Slash cooldown must be reduced to 0.04!");
        // Health preserved at 100 (no damage from bullet)
        assert_eq!(p.health, 100.0, "Player must take zero damage when slashing incoming bullet alongside enemy!");

        // Enemy is despawned via slice_event_resolution_system
        assert!(app.world().get_entity(enemy).is_err(), "Enemy must be despawned after slice!");
        // Bullet is despawned via katana slash
        assert!(app.world().get_entity(bullet).is_err(), "Bullet must be despawned after slash!");

        // Combo incremented for the enemy kill
        let combo = app.world().resource::<ComboState>();
        assert_eq!(combo.count, 1, "Combo must increment for the enemy kill!");
    }

    #[test]
    fn test_adversarial_zero_distance_bullet_aiming_no_nan() {
        use crate::*;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ChronoState>();
        app.add_event::<CombatAccuracyEvent>();
        app.add_systems(PreUpdate, |mut time: ResMut<Time<Real>>| {
            time.advance_by(std::time::Duration::from_millis(16));
        });
        app.add_systems(Update, enemy_ai_system);

        // Player placed exactly where gunner muzzle would be
        // muzzle = enemy_pos + (facing * 20, 14)
        // enemy_pos = (100, 100), facing = 1.0 => muzzle = (120, 114)
        // player_target = player_pos + (0, 16) => player_pos = (120, 98)
        let enemy_pos = Vec2::new(100.0, 100.0);
        let facing = 1.0;
        let muzzle = enemy_pos + Vec2::new(facing * 20.0, 14.0);
        let player_pos = muzzle - Vec2::new(0.0, 16.0);

        app.world_mut().spawn((
            Player::default(),
            Transform::from_xyz(player_pos.x, player_pos.y, 0.0),
        ));

        app.world_mut().spawn((
            Enemy {
                enemy_type: EnemyType::GunnerSemi,
                state: EnemyState::Attack { timer: 0.35, duration: 0.35, has_hit: false },
                velocity: Vec2::ZERO,
                facing,
                is_grounded: true,
                health: 1.0,
                max_health: 1.0,
                attack_range: 360.0,
                speed: 160.0,
                jump_force: 460.0,
                hitbox_radius: 18.0,
                attack_damage: 20.0,
                bullets_to_fire: 0,
                fire_cooldown: 0.0,
            },
            Transform::from_xyz(enemy_pos.x, enemy_pos.y, 0.0),
        ));

        // Update to fire bullet with zero target displacement
        app.update();

        let mut b_query = app.world_mut().query::<&Bullet>();
        let bullet = b_query.iter(app.world()).next().expect("Bullet should spawn");
        assert!(bullet.velocity.x.is_finite(), "Bullet velocity x must be finite!");
        assert!(bullet.velocity.y.is_finite(), "Bullet velocity y must be finite!");
        assert!(!bullet.velocity.x.is_nan(), "Bullet velocity must not be NaN!");
        assert!(!bullet.velocity.y.is_nan(), "Bullet velocity must not be NaN!");
        assert!(bullet.velocity.x > 0.0, "Velocity must default to facing direction (+x)!");
    }

    #[test]
    fn test_adversarial_burnout_exhaustive_matrix_and_overflow_protection() {
        use crate::chrono_shift::{Gear, GearAcclimatization};

        let gears = [
            Gear::N,
            Gear::G1,
            Gear::G2,
            Gear::G3,
            Gear::G4,
            Gear::G5,
            Gear::G6,
        ];

        // Test every combination of gear and mastered_tier (from 0 to 10)
        for m_tier in 0..=10 {
            let mut accl = GearAcclimatization::default();
            accl.mastered_tier = m_tier;

            for g in gears.iter() {
                let mult = accl.burnout_multiplier(*g);
                assert!(mult.is_finite(), "Multiplier must be finite for {:?} at tier {}", g, m_tier);
                assert!(mult >= 1.0, "Multiplier must be at least 1.0 baseline for {:?} at tier {}", g, m_tier);

                let idx = g.index();
                if idx == 0 {
                    assert_eq!(mult, 1.0, "Neutral gear must always have 1.0 multiplier!");
                } else if m_tier >= idx - 1 {
                    assert_eq!(mult, 1.0, "Gear {} must have 1.0 multiplier when tier {} >= {} - 1", idx, m_tier, idx);
                } else {
                    let skipped = (idx - 1 - m_tier) as f32;
                    let expected = 2.5 + skipped * 2.0 + (1.0 - accl.tolerance[idx]) * 2.0;
                    assert!((mult - expected).abs() < 1e-4, "Burnout formula match for Gear {} at tier {}", idx, m_tier);
                }
            }
        }

        // Test tolerance recovery invariants
        let mut accl = GearAcclimatization::default();
        accl.mastered_tier = 2;
        accl.tolerance[1] = 1.0;
        accl.tolerance[2] = 1.0;
        accl.tolerance[3] = 0.5;

        // Damage taken in G1 or G2 (already mastered) must NOT penalize tolerance
        accl.on_damage(Gear::G1);
        assert_eq!(accl.tolerance[1], 1.0, "Mastered gear tolerance must not decay on damage!");
        accl.on_miss(Gear::G2, false);
        assert_eq!(accl.tolerance[2], 1.0, "Mastered gear tolerance must not decay on miss!");

        // Damage taken in G3 (unmastered) does penalize tolerance
        accl.on_damage(Gear::G3);
        assert_eq!(accl.tolerance[3], 0.30, "Unmastered gear tolerance decays by 0.20 on damage!");

        // Miss in G3 with warp penalizes by 0.40 clamped at 0.0
        accl.on_miss(Gear::G3, true);
        assert_eq!(accl.tolerance[3], 0.0, "Unmastered gear tolerance clamps at 0.0 on penalty!");

        // Tolerance recovery: flawless kill restores tolerance
        accl.on_flawless_kill(Gear::G3, 1);
        assert!(accl.tolerance[3] > 0.0, "Tolerance recovers on flawless kill!");
    }
}
