use bevy::prelude::*;
use crate::player::{Player, ChronoEnergy};
use crate::combat::ComboState;
use crate::enemy::WaveManager;
use crate::time_control::ChronoState;

#[derive(Component)]
pub struct HealthBarFill;

#[derive(Component)]
pub struct EnergyBarFill;

#[derive(Component)]
pub struct EnergyText;

#[derive(Component)]
pub struct ScoreText;

#[derive(Component)]
pub struct ComboText;

#[derive(Component)]
pub struct WaveText;

#[derive(Component)]
pub struct SlowmoBanner;

#[derive(Component)]
#[allow(dead_code)]
pub struct GameOverOverlay;

pub fn setup_ui(mut commands: Commands) {
    // Root UI container
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            PickingBehavior::IGNORE,
        ))
        .with_children(|root| {
            // Top HUD row
            root.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(70.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|top_row| {
                // Top-Left: Health & Chrono Energy
                top_row
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|vbox| {
                        // Health Bar Container
                        vbox.spawn((
                            Node {
                                width: Val::Px(220.0),
                                height: Val::Px(16.0),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BorderColor(Color::srgba(0.8, 0.2, 0.2, 0.8)),
                            BackgroundColor(Color::srgba(0.1, 0.0, 0.0, 0.7)),
                        ))
                        .with_children(|bar| {
                            bar.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.95, 0.15, 0.15, 1.0)),
                                HealthBarFill,
                            ));
                        });

                        // Chrono Energy Bar Container
                        vbox.spawn((
                            Node {
                                width: Val::Px(220.0),
                                height: Val::Px(16.0),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BorderColor(Color::srgba(0.0, 0.8, 1.0, 0.8)),
                            BackgroundColor(Color::srgba(0.0, 0.1, 0.2, 0.7)),
                        ))
                        .with_children(|bar| {
                            bar.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgba(0.0, 0.9, 1.0, 1.0)),
                                EnergyBarFill,
                            ));
                        });

                        // Energy text percentage
                        vbox.spawn((
                            Text::new("CHRONO: 100%"),
                            TextFont {
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.2, 0.9, 1.0, 1.0)),
                            EnergyText,
                        ));
                    });

                // Top-Center: Wave indicator
                top_row
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|wave_box| {
                        wave_box.spawn((
                            Text::new("WAVE 1"),
                            TextFont {
                                font_size: 26.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            WaveText,
                        ));

                        // Slowmo notification
                        wave_box.spawn((
                            Text::new("<< CHRONO DILATION ACTIVE >>"),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.0, 1.0, 0.9, 0.0)),
                            SlowmoBanner,
                        ));
                    });

                // Top-Right: Score & Combo
                top_row
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexEnd,
                        ..default()
                    })
                    .with_children(|score_box| {
                        score_box.spawn((
                            Text::new("SCORE: 0"),
                            TextFont {
                                font_size: 22.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            ScoreText,
                        ));

                        score_box.spawn((
                            Text::new("COMBO x1"),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::srgba(1.0, 0.8, 0.2, 1.0)),
                            ComboText,
                        ));
                    });
            });

            // Bottom Controls HUD Guide
            root.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|bot| {
                bot.spawn((
                    Text::new(
                        "[A/D] Move   [W/Space] Jump   [E/MClick] Dash   [LClick] Slash   [Q/F] Warp   [Shift (Hold)] Stick Shift Time Dilator   [1-6 / Wheel / Drag] Gears",
                    ),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.7, 0.8, 0.9, 0.8)),
                ));
            });
        });
}

pub fn update_ui_system(
    player_q: Query<(&Player, &ChronoEnergy)>,
    combo: Res<ComboState>,
    wave_mgr: Res<WaveManager>,
    chrono: Res<ChronoState>,
    mut health_fill_q: Query<&mut Node, (With<HealthBarFill>, Without<EnergyBarFill>)>,
    mut energy_fill_q: Query<&mut Node, (With<EnergyBarFill>, Without<HealthBarFill>)>,
    mut energy_text_q: Query<&mut Text, (With<EnergyText>, Without<ScoreText>, Without<ComboText>, Without<WaveText>, Without<SlowmoBanner>)>,
    mut score_text_q: Query<&mut Text, (With<ScoreText>, Without<EnergyText>, Without<ComboText>, Without<WaveText>, Without<SlowmoBanner>)>,
    mut combo_text_q: Query<(&mut Text, &mut TextColor), (With<ComboText>, Without<EnergyText>, Without<ScoreText>, Without<WaveText>, Without<SlowmoBanner>)>,
    mut wave_text_q: Query<&mut Text, (With<WaveText>, Without<EnergyText>, Without<ScoreText>, Without<ComboText>, Without<SlowmoBanner>)>,
    mut slowmo_text_q: Query<&mut TextColor, (With<SlowmoBanner>, Without<ComboText>)>,
) {
    if let Ok((player, energy)) = player_q.get_single() {
        let hp_pct = (player.health / player.max_health * 100.0).clamp(0.0, 100.0);
        for mut node in health_fill_q.iter_mut() {
            node.width = Val::Percent(hp_pct);
        }

        let energy_pct = (energy.current / energy.max * 100.0).clamp(0.0, 100.0);
        for mut node in energy_fill_q.iter_mut() {
            node.width = Val::Percent(energy_pct);
        }

        for mut text in energy_text_q.iter_mut() {
            **text = format!("CHRONO: {:.0}%", energy_pct);
        }
    }

    for mut text in score_text_q.iter_mut() {
        **text = format!("SCORE: {}", combo.score);
    }

    for (mut text, mut color) in combo_text_q.iter_mut() {
        if combo.count > 1 {
            **text = format!("COMBO x{} 🔥 ({})", combo.multiplier, combo.count);
            let glow = match combo.multiplier {
                2 => Color::srgba(1.0, 0.9, 0.2, 1.0),
                3 => Color::srgba(1.0, 0.6, 0.1, 1.0),
                4 => Color::srgba(1.0, 0.2, 0.4, 1.0),
                _ => Color::srgba(0.8, 0.2, 1.0, 1.0),
            };
            *color = TextColor(glow);
        } else {
            **text = "COMBO x1".to_string();
            *color = TextColor(Color::srgba(0.6, 0.6, 0.6, 0.6));
        }
    }

    for mut text in wave_text_q.iter_mut() {
        match &wave_mgr.phase {
            crate::enemy::WavePhase::Intermission { timer } => {
                **text = format!("NEXT WAVE IN {:.1}s", timer);
            }
            crate::enemy::WavePhase::Spawning { remaining_to_spawn, .. } => {
                **text = format!("WAVE {} - {} REMAINING", wave_mgr.current_wave, remaining_to_spawn + wave_mgr.alive_count);
            }
            crate::enemy::WavePhase::Active => {
                **text = format!("WAVE {} - {} REMAINING", wave_mgr.current_wave, wave_mgr.alive_count);
            }
            crate::enemy::WavePhase::WaveCleared { .. } => {
                **text = format!("WAVE {} CLEARED!", wave_mgr.current_wave);
            }
        }
    }

    for mut color in slowmo_text_q.iter_mut() {
        if chrono.is_dilating {
            *color = TextColor(Color::srgba(0.0, 1.0, 0.9, 1.0));
        } else {
            *color = TextColor(Color::srgba(0.0, 1.0, 0.9, 0.0));
        }
    }
}
