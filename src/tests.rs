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
}
