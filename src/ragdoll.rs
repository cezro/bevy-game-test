use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RagdollPart {
    UpperTorso,
    LowerLegs,
}

#[derive(Component, Debug, Clone)]
pub struct RagdollPiece {
    pub part: RagdollPart,
    pub velocity: Vec2,
    pub angular_velocity: f32,
    pub rotation: f32,
    #[allow(dead_code)]
    pub facing: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub is_settled: bool,
    pub color: Color,
}

impl RagdollPiece {
    pub fn new(part: RagdollPart, velocity: Vec2, angular_velocity: f32, facing: f32, color: Color) -> Self {
        Self {
            part,
            velocity,
            angular_velocity,
            rotation: 0.0,
            facing,
            lifetime: 4.5,
            max_lifetime: 4.5,
            is_settled: false,
            color,
        }
    }
}

/// Particle component for blood/cyber-sparks flying from slice wounds
#[derive(Component, Debug, Clone)]
pub struct SliceParticle {
    pub velocity: Vec2,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub size: f32,
    pub color: Color,
    pub gravity_scale: f32,
}

/// Pure calculation: derives linear and angular velocities for both ragdoll halves after a slice.
pub fn calculate_slice_split_velocities(
    base_velocity: Vec2,
    slice_dir: Vec2,
    impulse: Vec2,
) -> (Vec2, f32, Vec2, f32) {
    let dir = if slice_dir.length_squared() > 1e-4 {
        slice_dir.normalize()
    } else {
        Vec2::Y
    };

    // Normal perpendicular to the slice plane
    let normal = Vec2::new(-dir.y, dir.x);

    // Upper half gets pushed upward and along normal
    let upper_vel = base_velocity * 0.4 + impulse + normal * 180.0 + Vec2::new(0.0, 140.0);
    let upper_ang_vel = (dir.x * -14.0) + (normal.y * 8.0);

    // Lower half gets pushed slightly downward/opposite and along base motion
    let lower_vel = base_velocity * 0.4 + impulse * 0.6 - normal * 140.0 + Vec2::new(0.0, 40.0);
    let lower_ang_vel = (dir.x * 12.0) - (normal.y * 6.0);

    (upper_vel, upper_ang_vel, lower_vel, lower_ang_vel)
}

/// Helper system to update ragdoll physics and particles
pub fn update_ragdoll_physics(
    time: Res<Time<Real>>,
    chrono: Res<crate::time_control::ChronoState>,
    mut ragdolls: Query<(Entity, &mut Transform, &mut RagdollPiece)>,
    mut particles: Query<(&mut Transform, &mut SliceParticle, Entity), Without<RagdollPiece>>,
    platforms: Query<&crate::player::Platform>,
    mut commands: Commands,
) {
    let dt = chrono.get_world_dt(time.delta_secs());
    if dt <= 0.0 {
        return;
    }

    let floor_y = -260.0;
    let gravity = 1200.0;

    for (entity, mut transform, mut piece) in ragdolls.iter_mut() {
        piece.lifetime -= dt;
        if piece.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        if piece.is_settled {
            continue;
        }

        // Apply gravity
        piece.velocity.y -= gravity * dt;
        transform.translation.x += piece.velocity.x * dt;
        transform.translation.y += piece.velocity.y * dt;
        piece.rotation += piece.angular_velocity * dt;
        transform.rotation = Quat::from_rotation_z(piece.rotation);

        // Ground / platform collision
        let feet_y = transform.translation.y;
        let mut landed = false;

        if feet_y <= floor_y {
            transform.translation.y = floor_y;
            landed = true;
        } else {
            // Check elevated platforms
            for plat in platforms.iter() {
                if transform.translation.x >= plat.min_x && transform.translation.x <= plat.max_x {
                    if feet_y <= plat.y && (feet_y - piece.velocity.y * dt) >= plat.y - 12.0 {
                        transform.translation.y = plat.y;
                        landed = true;
                        break;
                    }
                }
            }
        }

        if landed {
            if piece.velocity.y.abs() > 80.0 {
                // Bounce
                piece.velocity.y = -piece.velocity.y * 0.28;
                piece.velocity.x *= 0.75;
                piece.angular_velocity *= 0.6;
            } else {
                // Settle
                piece.velocity = Vec2::ZERO;
                piece.angular_velocity = 0.0;
                piece.is_settled = true;
            }
        }
    }

    // Update slice particles
    for (mut transform, mut part, entity) in particles.iter_mut() {
        part.lifetime -= dt;
        if part.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }

        part.velocity.y -= gravity * part.gravity_scale * dt;
        transform.translation.x += part.velocity.x * dt;
        transform.translation.y += part.velocity.y * dt;

        // Bounce on floor
        if transform.translation.y <= floor_y {
            transform.translation.y = floor_y;
            part.velocity.y = -part.velocity.y * 0.35;
            part.velocity.x *= 0.7;
        }
    }
}
