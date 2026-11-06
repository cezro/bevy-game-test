use bevy::prelude::*;
use crate::player::Player;
use crate::enemy::{Enemy, EnemyState, EnemyType};
use crate::ragdoll::{RagdollPiece, RagdollPart, SliceParticle};
use crate::combat::{SlashArcVisual, WarpBeamVisual, Bullet};

/// Joint points of a full stickman pose in world space
#[derive(Debug, Clone)]
pub struct StickmanPose {
    pub head_center: Vec2,
    pub head_radius: f32,
    pub neck: Vec2,
    pub pelvis: Vec2,
    // Arms
    pub left_shoulder: Vec2,
    pub left_elbow: Vec2,
    pub left_hand: Vec2,
    pub right_shoulder: Vec2,
    pub right_elbow: Vec2,
    pub right_hand: Vec2,
    // Legs
    pub left_hip: Vec2,
    pub left_knee: Vec2,
    pub left_foot: Vec2,
    pub right_hip: Vec2,
    pub right_knee: Vec2,
    pub right_foot: Vec2,
    // Weapon
    pub sword_hilt: Vec2,
    pub sword_tip: Vec2,
    // Scarf / Headband trail points
    pub scarf_nodes: [Vec2; 3],
}

/// Generates stickman pose based on position, facing, velocity, and animation cycle.
pub fn generate_stickman_pose(
    pos: Vec2,
    facing: f32,
    velocity: Vec2,
    is_grounded: bool,
    anim_time: f32,
    is_attacking: bool,
    attack_progress: f32,
    is_enemy: bool,
) -> StickmanPose {
    let speed = velocity.x.abs();
    let is_running = is_grounded && speed > 20.0;
    let is_jumping = !is_grounded;

    let base_pelvis = pos + Vec2::new(0.0, 16.0);
    let mut pelvis = base_pelvis;
    let mut neck = pelvis + Vec2::new(facing * 3.0, 24.0);

    // Torso lean
    if is_running {
        pelvis.y += (anim_time * 14.0).sin().abs() * 3.0;
        neck.x += facing * 6.0;
    } else if is_jumping {
        neck.x += facing * 4.0;
    }

    let head_center = neck + Vec2::new(facing * 3.0, 10.0);
    let head_radius = if is_enemy { 7.5 } else { 8.5 };

    // Legs
    let left_hip = pelvis + Vec2::new(-facing * 3.0, 0.0);
    let right_hip = pelvis + Vec2::new(facing * 3.0, 0.0);

    let (left_knee, left_foot, right_knee, right_foot) = if is_jumping {
        // Jumping / falling pose: legs tucked
        let l_k = left_hip + Vec2::new(-facing * 6.0, -10.0);
        let l_f = l_k + Vec2::new(-facing * 4.0, -10.0);
        let r_k = right_hip + Vec2::new(facing * 8.0, -8.0);
        let r_f = r_k + Vec2::new(facing * 6.0, -12.0);
        (l_k, l_f, r_k, r_f)
    } else if is_running {
        // Running stride
        let phase = anim_time * (speed * 0.04).clamp(8.0, 22.0);
        let l_angle = phase.sin();
        let r_angle = (phase + std::f32::consts::PI).sin();

        let l_k = left_hip + Vec2::new(l_angle * 12.0, -12.0 - l_angle.abs() * 3.0);
        let l_f = l_k + Vec2::new(l_angle * 10.0, -14.0);

        let r_k = right_hip + Vec2::new(r_angle * 12.0, -12.0 - r_angle.abs() * 3.0);
        let r_f = r_k + Vec2::new(r_angle * 10.0, -14.0);
        (l_k, l_f, r_k, r_f)
    } else {
        // Idle breathing stance
        let breath = (anim_time * 3.0).sin() * 1.5;
        let l_k = left_hip + Vec2::new(-facing * 4.0, -13.0 + breath);
        let l_f = l_k + Vec2::new(0.0, -13.0);
        let r_k = right_hip + Vec2::new(facing * 6.0, -12.0 + breath);
        let r_f = r_k + Vec2::new(0.0, -14.0);
        (l_k, l_f, r_k, r_f)
    };

    // Arms & Weapon
    let left_shoulder = neck + Vec2::new(-facing * 4.0, -2.0);
    let right_shoulder = neck + Vec2::new(facing * 4.0, -2.0);

    let (left_elbow, left_hand, right_elbow, right_hand, sword_hilt, sword_tip) = if is_attacking {
        // Forward slash swing pose
        let sweep = (1.0 - attack_progress).clamp(0.0, 1.0);
        let swing_angle = -0.8 + sweep * 2.2; // arc sweep
        let hand_dir = Vec2::new(facing * swing_angle.cos(), swing_angle.sin());

        let r_hand = right_shoulder + hand_dir * 22.0;
        let r_elbow = (right_shoulder + r_hand) * 0.5 + Vec2::new(0.0, 6.0);
        let l_hand = r_hand + Vec2::new(-facing * 4.0, -2.0);
        let l_elbow = (left_shoulder + l_hand) * 0.5;

        let blade_dir = hand_dir.normalize();
        let hilt = r_hand - blade_dir * 6.0;
        let tip = r_hand + blade_dir * 38.0;
        (l_elbow, l_hand, r_elbow, r_hand, hilt, tip)
    } else if is_running {
        // Pumping arms
        let arm_phase = anim_time * (speed * 0.04).clamp(8.0, 22.0);
        let l_swing = (arm_phase + std::f32::consts::PI).sin();
        let r_swing = arm_phase.sin();

        let l_elbow = left_shoulder + Vec2::new(l_swing * 8.0, -10.0);
        let l_hand = l_elbow + Vec2::new(l_swing * 6.0, -8.0);

        let r_elbow = right_shoulder + Vec2::new(r_swing * 8.0, -8.0);
        let r_hand = r_elbow + Vec2::new(facing * 10.0, 4.0);

        // Weapon resting or carried low
        let sword_dir = Vec2::new(facing * 0.8, -0.6).normalize();
        let hilt = r_hand - sword_dir * 4.0;
        let tip = r_hand + sword_dir * 32.0;
        (l_elbow, l_hand, r_elbow, r_hand, hilt, tip)
    } else {
        // Idle ready stance: holding katana across chest or at hip
        let l_elbow = left_shoulder + Vec2::new(-facing * 4.0, -10.0);
        let l_hand = pelvis + Vec2::new(facing * 4.0, 4.0);

        let r_elbow = right_shoulder + Vec2::new(facing * 6.0, -8.0);
        let r_hand = pelvis + Vec2::new(facing * 8.0, 6.0);

        let sword_dir = Vec2::new(facing * 0.7, 0.7).normalize();
        let hilt = r_hand - sword_dir * 5.0;
        let tip = r_hand + sword_dir * 34.0;
        (l_elbow, l_hand, r_elbow, r_hand, hilt, tip)
    };

    // Scarf / Headband ribbon
    let head_back = head_center + Vec2::new(-facing * head_radius, 0.0);
    let scarf_0 = head_back + Vec2::new(-facing * 8.0, ((anim_time * 8.0).sin() * 3.0) - velocity.y * 0.02);
    let scarf_1 = scarf_0 + Vec2::new(-facing * 10.0, ((anim_time * 8.0 + 1.2).sin() * 4.0) - velocity.y * 0.03);
    let scarf_2 = scarf_1 + Vec2::new(-facing * 12.0, ((anim_time * 8.0 + 2.4).sin() * 5.0) - velocity.y * 0.04);

    StickmanPose {
        head_center,
        head_radius,
        neck,
        pelvis,
        left_shoulder,
        left_elbow,
        left_hand,
        right_shoulder,
        right_elbow,
        right_hand,
        left_hip,
        left_knee,
        left_foot,
        right_hip,
        right_knee,
        right_foot,
        sword_hilt,
        sword_tip,
        scarf_nodes: [scarf_0, scarf_1, scarf_2],
    }
}

/// Afterimage ghost trail entity
#[derive(Component, Debug, Clone)]
pub struct GhostTrail {
    pub pose: StickmanPose,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub color: Color,
}

/// System to render all procedural stickmen, ghost trails, ragdolls, and FX via Gizmos
pub fn render_game_gizmos(
    mut gizmos: Gizmos,
    time: Res<Time<Real>>,
    player_q: Query<(&Transform, &Player)>,
    enemies_q: Query<(&Transform, &Enemy)>,
    ghosts_q: Query<&GhostTrail>,
    ragdolls_q: Query<(&Transform, &RagdollPiece)>,
    particles_q: Query<(&Transform, &SliceParticle)>,
    slash_q: Query<&SlashArcVisual>,
    warp_q: Query<&WarpBeamVisual>,
    bullets_q: Query<(&Transform, &Bullet)>,
    platforms_q: Query<&crate::player::Platform>,
) {
    let now = time.elapsed_secs();

    // 1. Draw Level Platforms & Floor
    let floor_color = Color::srgba(0.2, 0.25, 0.35, 1.0);
    let grid_cyan = Color::srgba(0.0, 0.85, 1.0, 0.6);
    let floor_y = -260.0;
    gizmos.line_2d(Vec2::new(-600.0, floor_y), Vec2::new(600.0, floor_y), floor_color);
    gizmos.line_2d(Vec2::new(-600.0, floor_y - 2.0), Vec2::new(600.0, floor_y - 2.0), grid_cyan);

    for plat in platforms_q.iter() {
        let p_start = Vec2::new(plat.min_x, plat.y);
        let p_end = Vec2::new(plat.max_x, plat.y);
        gizmos.line_2d(p_start, p_end, Color::srgba(0.3, 0.4, 0.55, 1.0));
        gizmos.line_2d(p_start + Vec2::new(0.0, -1.5), p_end + Vec2::new(0.0, -1.5), grid_cyan);
    }

    // 2. Draw Ghost Trails
    for ghost in ghosts_q.iter() {
        let alpha = (ghost.lifetime / ghost.max_lifetime).clamp(0.0, 1.0) * 0.45;
        let c = ghost.color.with_alpha(alpha);
        let p = &ghost.pose;

        // Draw ghost lines
        gizmos.circle_2d(p.head_center, p.head_radius, c);
        gizmos.line_2d(p.neck, p.pelvis, c);
        gizmos.line_2d(p.neck, p.left_elbow, c);
        gizmos.line_2d(p.left_elbow, p.left_hand, c);
        gizmos.line_2d(p.neck, p.right_elbow, c);
        gizmos.line_2d(p.right_elbow, p.right_hand, c);
        gizmos.line_2d(p.pelvis, p.left_knee, c);
        gizmos.line_2d(p.left_knee, p.left_foot, c);
        gizmos.line_2d(p.pelvis, p.right_knee, c);
        gizmos.line_2d(p.right_knee, p.right_foot, c);
        gizmos.line_2d(p.sword_hilt, p.sword_tip, c);
    }

    // 3. Draw Player
    for (transform, player) in player_q.iter() {
        let is_attacking = player.slash_cooldown > 0.08;
        let attack_prog = if player.slash_cooldown > 0.0 {
            player.slash_cooldown / 0.22
        } else {
            0.0
        };

        let pose = generate_stickman_pose(
            transform.translation.xy(),
            player.facing,
            player.velocity,
            player.is_grounded,
            now,
            is_attacking,
            attack_prog,
            false,
        );

        // Player styling: Crisp white body, electric cyan katana & headband
        let body_color = if player.invulnerability_timer > 0.0 && ((now * 25.0).sin() > 0.0) {
            Color::srgba(1.0, 1.0, 0.3, 0.9)
        } else {
            Color::WHITE
        };
        let katana_blade = Color::srgba(0.2, 0.9, 1.0, 1.0);
        let katana_glow = Color::srgba(0.6, 1.0, 1.0, 0.8);
        let scarf_color = Color::srgba(0.0, 0.8, 1.0, 0.9);

        // Head
        gizmos.circle_2d(pose.head_center, pose.head_radius, body_color);
        // Headband / Scarf
        let head_back = pose.head_center + Vec2::new(-player.facing * pose.head_radius, 0.0);
        gizmos.line_2d(head_back, pose.scarf_nodes[0], scarf_color);
        gizmos.line_2d(pose.scarf_nodes[0], pose.scarf_nodes[1], scarf_color);
        gizmos.line_2d(pose.scarf_nodes[1], pose.scarf_nodes[2], scarf_color);

        // Torso
        gizmos.line_2d(pose.neck, pose.pelvis, body_color);

        // Left Arm (back)
        gizmos.line_2d(pose.left_shoulder, pose.left_elbow, body_color);
        gizmos.line_2d(pose.left_elbow, pose.left_hand, body_color);

        // Legs
        gizmos.line_2d(pose.left_hip, pose.left_knee, body_color);
        gizmos.line_2d(pose.left_knee, pose.left_foot, body_color);
        gizmos.line_2d(pose.right_hip, pose.right_knee, body_color);
        gizmos.line_2d(pose.right_knee, pose.right_foot, body_color);

        // Right Arm (front)
        gizmos.line_2d(pose.right_shoulder, pose.right_elbow, body_color);
        gizmos.line_2d(pose.right_elbow, pose.right_hand, body_color);

        // Katana
        gizmos.line_2d(pose.sword_hilt, pose.sword_tip, katana_blade);
        gizmos.line_2d(pose.sword_hilt, pose.sword_tip, katana_glow);
    }

    // 4. Draw Enemies
    for (transform, enemy) in enemies_q.iter() {
        let (is_attacking, attack_prog, is_windup) = match enemy.state {
            EnemyState::Attack { timer, duration, .. } => (true, (timer / duration).clamp(0.0, 1.0), false),
            EnemyState::Windup { timer, duration } => (false, (timer / duration).clamp(0.0, 1.0), true),
            _ => (false, 0.0, false),
        };

        let pose = generate_stickman_pose(
            transform.translation.xy(),
            enemy.facing,
            enemy.velocity,
            enemy.is_grounded,
            now,
            is_attacking,
            attack_prog,
            true,
        );

        let (body_color, weapon_color) = match enemy.enemy_type {
            EnemyType::Runner => (Color::srgba(0.85, 0.2, 0.2, 1.0), Color::srgba(1.0, 0.3, 0.2, 1.0)),
            EnemyType::Ninja => (Color::srgba(0.5, 0.1, 0.7, 1.0), Color::srgba(0.8, 0.2, 1.0, 1.0)),
            EnemyType::Heavy => (Color::srgba(0.8, 0.35, 0.1, 1.0), Color::srgba(1.0, 0.5, 0.0, 1.0)),
            EnemyType::GunnerSemi => (Color::srgba(0.2, 0.8, 0.45, 1.0), Color::srgba(0.3, 1.0, 0.55, 1.0)),
            EnemyType::GunnerAuto => (Color::srgba(0.95, 0.7, 0.1, 1.0), Color::srgba(1.0, 0.85, 0.2, 1.0)),
        };

        // Windup telegraph line (laser warning)
        if is_windup {
            let warn_end = transform.translation.xy() + Vec2::new(enemy.facing * enemy.attack_range, 0.0);
            let warn_color = match enemy.enemy_type {
                EnemyType::GunnerSemi => Color::srgba(0.2, 1.0, 0.45, (now * 25.0).sin().abs()),
                EnemyType::GunnerAuto => Color::srgba(1.0, 0.8, 0.15, (now * 30.0).sin().abs()),
                _ => Color::srgba(1.0, 0.0, 0.0, (now * 20.0).sin().abs()),
            };
            gizmos.line_2d(
                transform.translation.xy() + Vec2::new(0.0, 10.0),
                warn_end,
                warn_color,
            );
        }

        // Head
        gizmos.circle_2d(pose.head_center, pose.head_radius, body_color);
        // Eye glow
        gizmos.circle_2d(
            pose.head_center + Vec2::new(enemy.facing * 3.0, 1.0),
            1.5,
            Color::srgba(1.0, 0.1, 0.1, 1.0),
        );

        // Torso
        gizmos.line_2d(pose.neck, pose.pelvis, body_color);

        // Arms & Legs
        gizmos.line_2d(pose.left_shoulder, pose.left_elbow, body_color);
        gizmos.line_2d(pose.left_elbow, pose.left_hand, body_color);
        gizmos.line_2d(pose.right_shoulder, pose.right_elbow, body_color);
        gizmos.line_2d(pose.right_elbow, pose.right_hand, body_color);
        gizmos.line_2d(pose.left_hip, pose.left_knee, body_color);
        gizmos.line_2d(pose.left_knee, pose.left_foot, body_color);
        gizmos.line_2d(pose.right_hip, pose.right_knee, body_color);
        gizmos.line_2d(pose.right_knee, pose.right_foot, body_color);

        // Weapon
        match enemy.enemy_type {
            EnemyType::GunnerSemi => {
                let gun_origin = pose.right_hand;
                let barrel_dir = Vec2::new(enemy.facing, 0.0);
                let barrel_end = gun_origin + barrel_dir * 26.0;
                // Rifle barrel & stock
                gizmos.line_2d(gun_origin - barrel_dir * 6.0, barrel_end, weapon_color);
                // Scope
                gizmos.line_2d(gun_origin + Vec2::new(0.0, 3.5), gun_origin + barrel_dir * 12.0 + Vec2::new(0.0, 3.5), weapon_color);
            }
            EnemyType::GunnerAuto => {
                let gun_origin = pose.right_hand;
                let barrel_dir = Vec2::new(enemy.facing, 0.0);
                let barrel_end = gun_origin + barrel_dir * 18.0;
                // Double barrel SMG
                gizmos.line_2d(gun_origin - barrel_dir * 4.0, barrel_end, weapon_color);
                gizmos.line_2d(gun_origin - barrel_dir * 4.0 + Vec2::new(0.0, 2.0), barrel_end + Vec2::new(0.0, 2.0), weapon_color);
                // Drum mag
                gizmos.circle_2d(gun_origin + barrel_dir * 4.0 + Vec2::new(0.0, -4.0), 3.0, weapon_color);
            }
            _ => {
                gizmos.line_2d(pose.sword_hilt, pose.sword_tip, weapon_color);
            }
        }
    }

    // 5. Draw Ragdoll Halves
    for (transform, piece) in ragdolls_q.iter() {
        let alpha = (piece.lifetime / piece.max_lifetime).clamp(0.0, 1.0);
        let c = piece.color.with_alpha(alpha);
        let p = transform.translation.xy();
        let rot = piece.rotation;
        let dir = Vec2::new(rot.cos(), rot.sin());
        let normal = Vec2::new(-dir.y, dir.x);

        match piece.part {
            RagdollPart::UpperTorso => {
                // Sliced waist to head
                let neck = p + normal * 14.0;
                let head = neck + normal * 8.0;
                gizmos.circle_2d(head, 7.5, c);
                gizmos.line_2d(p, neck, c);
                // Severed cut glow
                gizmos.line_2d(p - dir * 8.0, p + dir * 8.0, Color::srgba(1.0, 0.2, 0.1, alpha));
                // Flailing arms
                gizmos.line_2d(neck, neck + dir * 14.0 + normal * 4.0, c);
                gizmos.line_2d(neck, neck - dir * 14.0 + normal * 4.0, c);
            }
            RagdollPart::LowerLegs => {
                // Pelvis to feet
                let pelvis = p;
                let l_foot = pelvis - normal * 18.0 - dir * 8.0;
                let r_foot = pelvis - normal * 16.0 + dir * 10.0;
                gizmos.line_2d(pelvis, l_foot, c);
                gizmos.line_2d(pelvis, r_foot, c);
                // Severed cut glow
                gizmos.line_2d(p - dir * 7.0, p + dir * 7.0, Color::srgba(1.0, 0.2, 0.1, alpha));
            }
        }
    }

    // 6. Draw Slice Particles (Blood / Cyber Sparks)
    for (transform, particle) in particles_q.iter() {
        let alpha = (particle.lifetime / particle.max_lifetime).clamp(0.0, 1.0);
        let c = particle.color.with_alpha(alpha);
        let p = transform.translation.xy();
        let tail = p - particle.velocity.normalize_or_zero() * (particle.size * 2.5);
        gizmos.line_2d(tail, p, c);
    }

    // 7. Draw Slash Visuals
    for slash in slash_q.iter() {
        let alpha = (slash.timer / slash.duration).clamp(0.0, 1.0);
        let c = slash.color.with_alpha(alpha);
        // Draw 3 arc layers
        let segments = 10;
        let half_arc = slash.arc_span * 0.5;
        let mut prev_pt: Option<Vec2> = None;
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let ang = slash.angle - half_arc + t * slash.arc_span;
            let pt = slash.origin + Vec2::new(ang.cos(), ang.sin()) * slash.radius;
            if let Some(prev) = prev_pt {
                gizmos.line_2d(prev, pt, c);
            }
            prev_pt = Some(pt);
        }
    }

    // 8. Draw Warp Slice Beams
    for warp in warp_q.iter() {
        let alpha = (warp.timer / warp.duration).clamp(0.0, 1.0);
        let c = warp.color.with_alpha(alpha);
        let glow_c = Color::srgba(0.8, 0.95, 1.0, alpha);
        gizmos.line_2d(warp.start, warp.end, glow_c);
        gizmos.line_2d(warp.start + Vec2::new(0.0, 1.5), warp.end + Vec2::new(0.0, 1.5), c);
        gizmos.line_2d(warp.start - Vec2::new(0.0, 1.5), warp.end - Vec2::new(0.0, 1.5), c);
    }

    // 9. Draw Bullets (Luminous tracer lines and glowing cores)
    for (transform, bullet) in bullets_q.iter() {
        let pos = transform.translation.xy();
        let dir = bullet.velocity.normalize_or_zero();
        let tracer_len = if bullet.is_automatic { 16.0 } else { 24.0 };
        let tail = pos - dir * tracer_len;

        let (core_color, glow_color) = if bullet.is_automatic {
            (Color::srgba(1.0, 1.0, 0.6, 1.0), Color::srgba(1.0, 0.7, 0.1, 0.8))
        } else {
            (Color::srgba(0.8, 1.0, 0.9, 1.0), Color::srgba(0.2, 0.9, 0.45, 0.8))
        };

        // Outer tracer glow
        gizmos.line_2d(tail, pos, glow_color);
        // Inner luminous core
        gizmos.line_2d(tail + dir * (tracer_len * 0.4), pos, core_color);
        // Bullet head
        gizmos.circle_2d(pos, bullet.radius, core_color);
        gizmos.circle_2d(pos, bullet.radius + 1.5, glow_color);
    }
}
