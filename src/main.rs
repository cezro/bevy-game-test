use bevy::prelude::*;
use bevy::window::{Window, WindowPlugin, WindowResolution};

mod time_control;
mod player;
mod combat;
mod enemy;
mod ragdoll;
mod render;
mod ui;
pub mod chrono_shift;
#[cfg(test)]
mod tests;

use time_control::{ChronoState, update_chrono_system};
use player::{Player, ChronoEnergy, Platform, step_horizontal_velocity, step_vertical_velocity};
use combat::{ComboState, SliceEnemyEvent, SlashArcVisual, WarpBeamVisual, segment_intersects_circle, katana_arc_intersects};
use enemy::{Enemy, EnemyState, EnemyType, WaveManager, WaveAction, step_enemy_ai};
use ragdoll::{RagdollPiece, RagdollPart, SliceParticle, calculate_slice_split_velocities, update_ragdoll_physics};
use render::{render_game_gizmos, GhostTrail, generate_stickman_pose};
use ui::{setup_ui, update_ui_system};
use chrono_shift::{ChronoShiftPlugin, ChronoShifter, CombatAccuracyEvent};

#[derive(Resource, Default)]
pub struct CursorWorldCoords(pub Option<Vec2>);

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource)]
pub struct GameAudioAssets {
    pub slash: Handle<AudioSource>,
    pub warp: Handle<AudioSource>,
    pub slowmo_enter: Handle<AudioSource>,
    pub slowmo_exit: Handle<AudioSource>,
    pub slice_kill: Handle<AudioSource>,
    pub dash: Handle<AudioSource>,
    pub jump: Handle<AudioSource>,
}

fn setup_audio_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(GameAudioAssets {
        slash: asset_server.load("audio/slash.wav"),
        warp: asset_server.load("audio/warp.wav"),
        slowmo_enter: asset_server.load("audio/slowmo_enter.wav"),
        slowmo_exit: asset_server.load("audio/slowmo_exit.wav"),
        slice_kill: asset_server.load("audio/slice_kill.wav"),
        dash: asset_server.load("audio/dash.wav"),
        jump: asset_server.load("audio/jump.wav"),
    });
}


fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Chrono Stick - Stickman Time-Manipulating Action".to_string(),
                resolution: WindowResolution::new(1280.0, 720.0),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<ChronoState>()
        .init_resource::<ComboState>()
        .init_resource::<WaveManager>()
        .init_resource::<CursorWorldCoords>()
        .add_event::<SliceEnemyEvent>()
        .add_plugins(ChronoShiftPlugin)
        .add_systems(Startup, (setup_level, setup_ui, setup_audio_assets))
        .add_systems(
            Update,
            (
                update_chrono_system,
                update_cursor_system,
                player_input_and_movement_system,
                player_katana_slash_system,
                player_chrono_warp_system,
                enemy_ai_system,
                wave_manager_system,
                slice_event_resolution_system,
                update_visual_fx_system,
                update_ragdoll_physics,
                update_camera_shake_system,
                update_ui_system,
                game_restart_system,
            ),
        )
        .add_systems(PostUpdate, render_game_gizmos)
        .run();
}

fn setup_level(mut commands: Commands) {
    // 2D Camera
    commands.spawn((
        Camera2d,
        MainCamera,
    ));

    // Spawn Player
    commands.spawn((
        Player::default(),
        ChronoEnergy::default(),
        Transform::from_xyz(0.0, -200.0, 10.0),
        Visibility::default(),
    ));

    // Spawn Arena Platforms
    // Left platform
    commands.spawn((
        Platform::new(-300.0, -120.0, 240.0),
        Transform::from_xyz(-300.0, -120.0, 0.0),
    ));

    // Right platform
    commands.spawn((
        Platform::new(300.0, -120.0, 240.0),
        Transform::from_xyz(300.0, -120.0, 0.0),
    ));

    // Center elevated high platform
    commands.spawn((
        Platform::new(0.0, 20.0, 200.0),
        Transform::from_xyz(0.0, 20.0, 0.0),
    ));
}

pub fn update_cursor_system(
    mut cursor_coords: ResMut<CursorWorldCoords>,
    q_window: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    shifter: Option<Res<ChronoShifter>>,
) {
    if let Some(ref s) = shifter {
        if s.aim_frozen() {
            return;
        }
    }

    let Ok(window) = q_window.get_single() else {
        cursor_coords.0 = None;
        return;
    };
    let Ok((camera, camera_transform)) = q_camera.get_single() else {
        cursor_coords.0 = None;
        return;
    };

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            cursor_coords.0 = Some(world_pos);
            return;
        }
    }
    cursor_coords.0 = None;
}

fn update_camera_shake_system(
    chrono: Res<ChronoState>,
    mut camera_q: Query<&mut Transform, With<MainCamera>>,
) {
    for mut transform in camera_q.iter_mut() {
        transform.translation.x = chrono.shake_offset.x;
        transform.translation.y = chrono.shake_offset.y;
    }
}

pub fn player_input_and_movement_system(
    time: Res<Time<Real>>,
    chrono: Res<ChronoState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorWorldCoords>,
    mut player_q: Query<(&mut Transform, &mut Player)>,
    platforms_q: Query<&Platform>,
    audio: Option<Res<GameAudioAssets>>,
    mut commands: Commands,
) {
    let Ok((mut transform, mut player)) = player_q.get_single_mut() else {
        return;
    };

    let real_dt = time.delta_secs();

    // 1. Time Dilation is driven dynamically by the Chronos Shift stick-shift (Shift hold)
    // and meter drain/regen is handled by ChronoShiftPlugin before this system runs.

    let dt = chrono.get_player_dt(real_dt);
    if dt <= 0.0 {
        return;
    }

    // Update cooldowns
    if player.slash_cooldown > 0.0 {
        player.slash_cooldown = (player.slash_cooldown - dt).max(0.0);
    }
    if player.warp_cooldown > 0.0 {
        player.warp_cooldown = (player.warp_cooldown - dt).max(0.0);
    }
    if player.dash_cooldown > 0.0 {
        player.dash_cooldown = (player.dash_cooldown - dt).max(0.0);
    }
    if player.invulnerability_timer > 0.0 {
        player.invulnerability_timer = (player.invulnerability_timer - dt).max(0.0);
    }

    // Aim direction & facing
    if let Some(cursor_pos) = cursor.0 {
        if (cursor_pos.x - transform.translation.x).abs() > 8.0 {
            player.facing = if cursor_pos.x >= transform.translation.x { 1.0 } else { -1.0 };
        }
    }

    // 2. Dash Movement: KeyCode::KeyE or MouseButton::Middle
    let dash_key = keyboard.just_pressed(KeyCode::KeyE) || mouse_buttons.just_pressed(MouseButton::Middle);
    if dash_key && player.dash_cooldown <= 0.0 {
        if let Some(ref sfx) = audio {
            commands.spawn((AudioPlayer::new(sfx.dash.clone()), PlaybackSettings::DESPAWN));
        }
        player.dash_timer = player.config.dash_duration;
        player.dash_cooldown = player.config.dash_cooldown_max;
        player.invulnerability_timer = player.config.dash_duration;
        let mut dash_dir_x = 0.0;
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            dash_dir_x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            dash_dir_x += 1.0;
        }
        if dash_dir_x == 0.0 {
            dash_dir_x = player.facing;
        }
        player.dash_dir = Vec2::new(dash_dir_x, 0.0).normalize();

        // Spawn afterimage ghost trail
        let pose = generate_stickman_pose(
            transform.translation.xy(),
            player.facing,
            player.velocity,
            player.is_grounded,
            time.elapsed_secs(),
            false,
            0.0,
            false,
        );
        commands.spawn(GhostTrail {
            pose,
            lifetime: 0.35,
            max_lifetime: 0.35,
            color: Color::srgba(0.0, 0.9, 1.0, 0.8),
        });
    }

    // Handle Dash state
    if player.dash_timer > 0.0 {
        player.dash_timer -= dt;
        player.velocity = player.dash_dir * player.config.dash_speed;
        transform.translation.x += player.velocity.x * dt;
        // Clamp to screen bounds
        transform.translation.x = transform.translation.x.clamp(-580.0, 580.0);
        return;
    }

    // 3. Horizontal Kinematics (Crisp acceleration & deceleration)
    let mut input_x = 0.0;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        input_x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        input_x += 1.0;
    }

    player.velocity.x = step_horizontal_velocity(player.velocity.x, input_x, &player.config, dt);

    // 4. Vertical Kinematics (Variable jump height, Coyote time, Jump buffer)
    let jump_pressed = keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::ArrowUp);
    let jump_held = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::Space) || keyboard.pressed(KeyCode::ArrowUp);

    let (new_vy, new_coyote, new_buffer, did_jump) = step_vertical_velocity(
        player.velocity.y,
        player.is_grounded,
        jump_held,
        jump_pressed,
        player.coyote_timer,
        player.jump_buffer_timer,
        &player.config,
        dt,
    );
    player.velocity.y = new_vy;
    player.coyote_timer = new_coyote;
    player.jump_buffer_timer = new_buffer;
    if did_jump {
        if let Some(ref sfx) = audio {
            commands.spawn((AudioPlayer::new(sfx.jump.clone()), PlaybackSettings::DESPAWN));
        }
    }

    // 5. Apply velocities
    let prev_y = transform.translation.y;
    transform.translation.x += player.velocity.x * dt;
    transform.translation.y += player.velocity.y * dt;

    // Screen bounds clamp
    transform.translation.x = transform.translation.x.clamp(-580.0, 580.0);

    // 6. Platform & Ground Collisions
    let floor_y = -260.0;
    player.is_grounded = false;

    // Floor check
    if transform.translation.y <= floor_y {
        transform.translation.y = floor_y;
        player.velocity.y = 0.0;
        player.is_grounded = true;
    } else if player.velocity.y <= 0.0 {
        // Platform top landing check
        for plat in platforms_q.iter() {
            if transform.translation.x >= plat.min_x && transform.translation.x <= plat.max_x {
                if prev_y >= plat.y && transform.translation.y <= plat.y {
                    transform.translation.y = plat.y;
                    player.velocity.y = 0.0;
                    player.is_grounded = true;
                    break;
                }
            }
        }
    }
}

pub fn player_katana_slash_system(
    _time: Res<Time<Real>>,
    chrono: Res<ChronoState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorWorldCoords>,
    shifter: Option<Res<ChronoShifter>>,
    mut player_q: Query<(&Transform, &mut Player), Without<Enemy>>,
    enemies_q: Query<(Entity, &Transform, &Enemy), Without<Player>>,
    audio: Option<Res<GameAudioAssets>>,
    mut slice_events: EventWriter<SliceEnemyEvent>,
    mut accuracy_events: EventWriter<CombatAccuracyEvent>,
    mut commands: Commands,
) {
    if let Some(ref s) = shifter {
        if s.is_held() {
            return;
        }
    }

    let Ok((p_transform, mut player)) = player_q.get_single_mut() else {
        return;
    };

    if chrono.is_hitstop_active() {
        return;
    }

    if mouse_buttons.just_pressed(MouseButton::Left) && player.slash_cooldown <= 0.0 {
        player.slash_cooldown = 0.22;
        if let Some(ref sfx) = audio {
            commands.spawn((AudioPlayer::new(sfx.slash.clone()), PlaybackSettings::DESPAWN));
        }
        let origin = p_transform.translation.xy() + Vec2::new(0.0, 16.0);

        let aim_dir = if let Some(cursor_pos) = cursor.0 {
            (cursor_pos - origin).normalize_or_zero()
        } else {
            Vec2::new(player.facing, 0.0)
        };

        let slash_angle = aim_dir.y.atan2(aim_dir.x);
        let slash_range = 80.0;
        let arc_span = std::f32::consts::PI * 0.75; // 135 deg

        // Spawn visual slash arc
        commands.spawn(SlashArcVisual {
            origin,
            angle: slash_angle,
            arc_span,
            radius: slash_range,
            timer: 0.14,
            duration: 0.14,
            color: Color::srgba(0.2, 0.95, 1.0, 1.0),
        });

        // Check enemies hit by katana slash
        let mut hit_count = 0;
        for (e_entity, e_transform, enemy) in enemies_q.iter() {
            let enemy_center = e_transform.translation.xy() + Vec2::new(0.0, 16.0);
            if katana_arc_intersects(origin, player.facing, aim_dir, enemy_center, slash_range, arc_span, enemy.hitbox_radius) {
                hit_count += 1;
                let slice_dir = aim_dir;
                let impulse = aim_dir * 320.0 + Vec2::new(0.0, 180.0);
                slice_events.send(SliceEnemyEvent {
                    enemy_entity: e_entity,
                    slice_position: enemy_center,
                    slice_direction: slice_dir,
                    impulse,
                    is_warp_kill: false,
                });
            }
        }

        if hit_count > 0 {
            accuracy_events.send(CombatAccuracyEvent::FlawlessKill { count: hit_count, is_warp: false });
        } else {
            accuracy_events.send(CombatAccuracyEvent::Miss { is_warp: false });
        }
    }
}

pub fn player_chrono_warp_system(
    time: Res<Time<Real>>,
    chrono: Res<ChronoState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    cursor: Res<CursorWorldCoords>,
    mut player_q: Query<(&mut Transform, &mut Player, &mut ChronoEnergy), Without<Enemy>>,
    enemies_q: Query<(Entity, &Transform, &Enemy), Without<Player>>,
    audio: Option<Res<GameAudioAssets>>,
    mut slice_events: EventWriter<SliceEnemyEvent>,
    mut accuracy_events: EventWriter<CombatAccuracyEvent>,
    mut commands: Commands,
) {
    let Ok((mut p_transform, mut player, mut energy)) = player_q.get_single_mut() else {
        return;
    };

    if chrono.is_hitstop_active() {
        return;
    }

    let warp_pressed = keyboard.just_pressed(KeyCode::KeyQ)
        || keyboard.just_pressed(KeyCode::KeyF);

    if warp_pressed && player.warp_cooldown <= 0.0 && energy.can_teleport() {
        let Some(cursor_pos) = cursor.0 else {
            return;
        };

        energy.consume_teleport();
        player.warp_cooldown = 0.20;
        player.invulnerability_timer = 0.25;
        if let Some(ref sfx) = audio {
            commands.spawn((AudioPlayer::new(sfx.warp.clone()), PlaybackSettings::DESPAWN));
        }

        let start_pos = p_transform.translation.xy();
        let warp_vector = cursor_pos - start_pos;
        let max_range = 380.0;
        let dist = warp_vector.length();
        let target_pos = if dist > max_range {
            start_pos + (warp_vector / dist) * max_range
        } else {
            cursor_pos
        };

        // Bounded target
        let clamped_target = Vec2::new(
            target_pos.x.clamp(-570.0, 570.0),
            target_pos.y.clamp(-260.0, 320.0),
        );

        let slice_dir = (clamped_target - start_pos).normalize_or_zero();

        // 1. Instantly teleport player
        p_transform.translation.x = clamped_target.x;
        p_transform.translation.y = clamped_target.y;
        player.velocity = Vec2::ZERO;

        // 2. Spawn Neon Warp Slice Beam
        commands.spawn(WarpBeamVisual {
            start: start_pos + Vec2::new(0.0, 16.0),
            end: clamped_target + Vec2::new(0.0, 16.0),
            timer: 0.25,
            duration: 0.25,
            color: Color::srgba(0.0, 1.0, 0.9, 1.0),
        });

        // 3. Spawn Afterimage Ghost Trails along teleport trajectory
        for i in 1..=3 {
            let t = i as f32 / 4.0;
            let mid_pos = start_pos.lerp(clamped_target, t);
            let pose = generate_stickman_pose(
                mid_pos,
                player.facing,
                slice_dir * 500.0,
                false,
                time.elapsed_secs(),
                true,
                0.2,
                false,
            );
            commands.spawn(GhostTrail {
                pose,
                lifetime: 0.35 + (i as f32 * 0.05),
                max_lifetime: 0.35 + (i as f32 * 0.05),
                color: Color::srgba(0.0, 0.9, 1.0, 0.9),
            });
        }

        // 4. Test all enemies along the line segment
        let seg_start = start_pos + Vec2::new(0.0, 16.0);
        let seg_end = clamped_target + Vec2::new(0.0, 16.0);
        let slice_radius = 28.0;

        let mut hit_count = 0;
        for (e_entity, e_transform, enemy) in enemies_q.iter() {
            let enemy_center = e_transform.translation.xy() + Vec2::new(0.0, 16.0);
            if segment_intersects_circle(seg_start, seg_end, enemy_center, enemy.hitbox_radius + slice_radius) {
                hit_count += 1;
                let impulse = slice_dir * 450.0 + Vec2::new(0.0, 240.0);
                slice_events.send(SliceEnemyEvent {
                    enemy_entity: e_entity,
                    slice_position: enemy_center,
                    slice_direction: slice_dir,
                    impulse,
                    is_warp_kill: true,
                });
            }
        }

        if hit_count > 0 {
            accuracy_events.send(CombatAccuracyEvent::FlawlessKill { count: hit_count, is_warp: true });
        } else {
            accuracy_events.send(CombatAccuracyEvent::Miss { is_warp: true });
        }
    }
}

pub fn slice_event_resolution_system(
    mut slice_events: EventReader<SliceEnemyEvent>,
    mut chrono: ResMut<ChronoState>,
    mut combo: ResMut<ComboState>,
    mut wave_mgr: ResMut<WaveManager>,
    mut player_q: Query<&mut ChronoEnergy>,
    enemies_q: Query<(&Enemy, &Transform)>,
    audio: Option<Res<GameAudioAssets>>,
    mut commands: Commands,
) {
    let mut processed = std::collections::HashSet::new();

    for ev in slice_events.read() {
        if !processed.insert(ev.enemy_entity) {
            continue;
        }

        let Ok((enemy, _transform)) = enemies_q.get(ev.enemy_entity) else {
            continue;
        };

        // 1. Trigger Hitstop & Screen Shake
        let hitstop = if ev.is_warp_kill {
            ChronoState::HITSTOP_WARP
        } else {
            ChronoState::HITSTOP_NORMAL
        };
        let shake = if ev.is_warp_kill { 12.0 } else { 7.0 };
        chrono.trigger_hitstop(hitstop);
        chrono.trigger_shake(shake);
        if let Some(ref sfx) = audio {
            commands.spawn((AudioPlayer::new(sfx.slice_kill.clone()), PlaybackSettings::DESPAWN));
        }

        // 2. Score & Combo
        combo.register_kill(ev.is_warp_kill);

        // 3. Chrono Energy reward
        if let Ok(mut energy) = player_q.get_single_mut() {
            energy.reward_kill();
        }

        // 4. Wave manager update
        wave_mgr.on_enemy_killed();

        // 5. Ragdoll Halves Split
        let (upper_vel, upper_rot, lower_vel, lower_rot) =
            calculate_slice_split_velocities(enemy.velocity, ev.slice_direction, ev.impulse);

        let ragdoll_color = match enemy.enemy_type {
            EnemyType::Runner => Color::srgba(0.85, 0.2, 0.2, 1.0),
            EnemyType::Ninja => Color::srgba(0.55, 0.1, 0.75, 1.0),
            EnemyType::Heavy => Color::srgba(0.85, 0.4, 0.1, 1.0),
        };

        // Upper Ragdoll
        let upper_piece = RagdollPiece::new(
            RagdollPart::UpperTorso,
            upper_vel,
            upper_rot,
            enemy.facing,
            ragdoll_color,
        );
        commands.spawn((
            upper_piece,
            Transform::from_translation(Vec3::new(ev.slice_position.x, ev.slice_position.y + 10.0, 0.0)),
        ));

        // Lower Ragdoll
        let lower_piece = RagdollPiece::new(
            RagdollPart::LowerLegs,
            lower_vel,
            lower_rot,
            enemy.facing,
            ragdoll_color,
        );
        commands.spawn((
            lower_piece,
            Transform::from_translation(Vec3::new(ev.slice_position.x, ev.slice_position.y, 0.0)),
        ));

        // 6. Blood & Cyber Sparks
        let spark_count = if ev.is_warp_kill { 24 } else { 14 };
        for i in 0..spark_count {
            let angle = (i as f32 / spark_count as f32) * std::f32::consts::TAU;
            let speed = 120.0 + (i as f32 * 27.0) % 240.0;
            let spark_dir = Vec2::new(angle.cos(), angle.sin());
            let spark_color = if ev.is_warp_kill {
                Color::srgba(0.1, 0.95, 1.0, 1.0) // Electric cyan sparks for warp
            } else {
                Color::srgba(0.95, 0.15, 0.2, 1.0) // Red gore sparks
            };

            commands.spawn((
                SliceParticle {
                    velocity: spark_dir * speed + ev.slice_direction * 150.0,
                    lifetime: 0.6 + (i as f32 * 0.04),
                    max_lifetime: 0.6 + (i as f32 * 0.04),
                    size: 3.5,
                    color: spark_color,
                    gravity_scale: 0.8,
                },
                Transform::from_translation(Vec3::new(ev.slice_position.x, ev.slice_position.y, 1.0)),
            ));
        }

        // 7. Despawn enemy
        commands.entity(ev.enemy_entity).despawn();
    }
}

fn enemy_ai_system(
    time: Res<Time<Real>>,
    chrono: Res<ChronoState>,
    mut player_q: Query<(&Transform, &mut Player), Without<Enemy>>,
    mut enemies_q: Query<(Entity, &mut Transform, &mut Enemy), Without<Player>>,
    platforms_q: Query<&Platform>,
    mut accuracy_events: EventWriter<CombatAccuracyEvent>,
) {
    let dt = chrono.get_world_dt(time.delta_secs());
    if dt <= 0.0 {
        return;
    }

    let Ok((p_transform, mut player)) = player_q.get_single_mut() else {
        return;
    };
    let player_pos = p_transform.translation.xy();
    let floor_y = -260.0;
    let gravity = 1200.0;

    for (_e_entity, mut e_transform, mut enemy) in enemies_q.iter_mut() {
        let enemy_pos = e_transform.translation.xy();

        // 1. AI Decision Step
        let (next_state, move_dir, should_jump) = step_enemy_ai(
            enemy_pos,
            player_pos,
            &enemy.state,
            enemy.enemy_type,
            enemy.attack_range,
            enemy.is_grounded,
            dt,
        );

        enemy.state = next_state;
        if (player_pos.x - enemy_pos.x).abs() > 4.0 {
            enemy.facing = if player_pos.x >= enemy_pos.x { 1.0 } else { -1.0 };
        }

        // 2. Jumping
        if should_jump && enemy.is_grounded {
            enemy.velocity.y = enemy.jump_force;
            enemy.is_grounded = false;
        }

        // 3. Movement & Gravity
        enemy.velocity.x = move_dir * enemy.speed;
        enemy.velocity.y -= gravity * dt;

        let prev_y = e_transform.translation.y;
        e_transform.translation.x += enemy.velocity.x * dt;
        e_transform.translation.y += enemy.velocity.y * dt;

        // Clamp to screen bounds and arena floor
        e_transform.translation.x = e_transform.translation.x.clamp(-580.0, 580.0);
        if e_transform.translation.y < -350.0 {
            e_transform.translation.y = floor_y;
            enemy.velocity.y = 0.0;
            enemy.is_grounded = true;
        }

        // 4. Platform and Floor Collision
        enemy.is_grounded = false;
        if e_transform.translation.y <= floor_y {
            e_transform.translation.y = floor_y;
            enemy.velocity.y = 0.0;
            enemy.is_grounded = true;
        } else if enemy.velocity.y <= 0.0 {
            for plat in platforms_q.iter() {
                if e_transform.translation.x >= plat.min_x && e_transform.translation.x <= plat.max_x {
                    if prev_y >= plat.y && e_transform.translation.y <= plat.y {
                        e_transform.translation.y = plat.y;
                        enemy.velocity.y = 0.0;
                        enemy.is_grounded = true;
                        break;
                    }
                }
            }
        }

        // 5. Attack Hitbox Execution
        let facing = enemy.facing;
        let attack_range = enemy.attack_range;
        let hitbox_radius = enemy.hitbox_radius;
        let damage = enemy.attack_damage;

        if let EnemyState::Attack { ref mut has_hit, .. } = enemy.state {
            if !*has_hit {
                let attack_center = enemy_pos + Vec2::new(facing * attack_range * 0.6, 12.0);
                if (attack_center - (player_pos + Vec2::new(0.0, 16.0))).length() < (hitbox_radius + 20.0) {
                    *has_hit = true;
                    if player.invulnerability_timer <= 0.0 {
                        player.health = (player.health - damage).max(0.0);
                        player.invulnerability_timer = 0.45;
                        accuracy_events.send(CombatAccuracyEvent::TookDamage);
                    }
                }
            }
        }
    }
}

fn wave_manager_system(
    time: Res<Time<Real>>,
    chrono: Res<ChronoState>,
    mut wave_mgr: ResMut<WaveManager>,
    mut commands: Commands,
) {
    let dt = chrono.get_world_dt(time.delta_secs());
    if dt <= 0.0 {
        return;
    }

    let action = wave_mgr.update(dt, 1160.0, -250.0);
    if let Some(act) = action {
        match act {
            WaveAction::SpawnEnemy(e_type, spawn_pos) => {
                commands.spawn((
                    Enemy::new(e_type),
                    Transform::from_xyz(spawn_pos.x, spawn_pos.y, 5.0),
                    Visibility::default(),
                ));
            }
            WaveAction::AnnounceWave(_) => {}
            WaveAction::AnnounceClear(_) => {}
        }
    }
}

fn update_visual_fx_system(
    time: Res<Time<Real>>,
    mut ghosts_q: Query<(&mut GhostTrail, Entity)>,
    mut slash_q: Query<(&mut SlashArcVisual, Entity)>,
    mut warp_q: Query<(&mut WarpBeamVisual, Entity)>,
    mut combo: ResMut<ComboState>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    // Combo timer decay
    combo.update(dt);

    // Ghost trails
    for (mut ghost, entity) in ghosts_q.iter_mut() {
        ghost.lifetime -= dt;
        if ghost.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        }
    }

    // Slash visual arcs
    for (mut slash, entity) in slash_q.iter_mut() {
        slash.timer -= dt;
        if slash.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }

    // Warp slice beams
    for (mut warp, entity) in warp_q.iter_mut() {
        warp.timer -= dt;
        if warp.timer <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn game_restart_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_q: Query<(&mut Transform, &mut Player, &mut ChronoEnergy)>,
    enemies_q: Query<Entity, With<Enemy>>,
    ragdolls_q: Query<Entity, With<RagdollPiece>>,
    particles_q: Query<Entity, With<SliceParticle>>,
    mut wave_mgr: ResMut<WaveManager>,
    mut combo: ResMut<ComboState>,
    mut chrono: ResMut<ChronoState>,
    shifter: Option<ResMut<ChronoShifter>>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        if let Ok((mut transform, mut player, mut energy)) = player_q.get_single_mut() {
            transform.translation = Vec3::new(0.0, -200.0, 10.0);
            player.health = 100.0;
            player.velocity = Vec2::ZERO;
            energy.current = 100.0;
        }

        // Despawn all enemies, ragdolls, particles
        for e in enemies_q.iter() {
            commands.entity(e).despawn();
        }
        for r in ragdolls_q.iter() {
            commands.entity(r).despawn();
        }
        for p in particles_q.iter() {
            commands.entity(p).despawn();
        }

        // Reset managers
        *wave_mgr = WaveManager::default();
        combo.reset();
        *chrono = ChronoState::default();
        if let Some(mut s) = shifter {
            *s = ChronoShifter::default();
        }
    }
}
