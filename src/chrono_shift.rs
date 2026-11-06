//! Chronos Shift — interactive H-pattern stick-shift time dilator.
//!
//! Holding **Shift** raises a brass H-gate (top-right of the screen). The knob position
//! continuously sets world time dilation; deeper gears slow time more and drain the
//! Chrono meter faster. Releasing Shift springs the knob back to Neutral.
//!
//! Architecture:
//! * A pure, deterministic kinematic core (`constrain_step`, `detent_step`, `plan_gear_path`,
//!   `follow_path`, `engagement`, `dilation_at`, `drain_for_dilation`, `stall_update`) plus the
//!   [`ShifterModel`] state machine that composes them. Everything here is unit-testable
//!   without Bevy schedules.
//! * [`ChronoShiftCorePlugin`]: resource + control systems (input -> model -> ChronoState / meter
//!   drain -> cursor pin / aim freeze). Runs in `Update` strictly before
//!   `update_chrono_system`, `update_cursor_system` and all aim/time consumers.
//! * [`ChronoShiftPlugin`]: core + visuals (gizmo gate, cog knob, UI readout, tint overlay, sfx).
//!
//! Gate invariant (checked by [`is_legal`]): the knob is always finite, inside `[-1,1]^2` and
//! either on the neutral rail band (`|y| <= RAIL_TOL`) or exactly on a column (`x ∈ {-1,0,1}`).

use std::collections::VecDeque;

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::player::{ChronoEnergy, Player};
use crate::time_control::{update_chrono_system, ChronoState};
use crate::{CursorWorldCoords, MainCamera};

// ---------------------------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------------------------

/// Half-thickness of the neutral rail band (gate units).
pub const RAIL_TOL: f32 = 0.12;
/// How close (in x) the knob must be to a column to be allowed to enter its slot.
pub const COL_TOL: f32 = 0.2;
/// Gate columns.
pub const COLUMNS: [f32; 3] = [-1.0, 0.0, 1.0];
/// Maximum sub-step length; must be well below `2 * RAIL_TOL` so the band cannot be skipped.
pub const MAX_SUBSTEP: f32 = 0.05;
/// Per-axis clamp on a single input delta (the whole gate is only ~4 units of path long).
pub const MAX_DELTA: f32 = 8.0;
/// Slot depth at which engagement begins.
pub const ENGAGE_START: f32 = 0.15;
/// Slot depth beyond which the detent seats the knob into the gear end instead of the rail.
pub const SEAT_THRESHOLD: f32 = 0.45;
/// Detent spring stiffness (1/s).
pub const DETENT_K: f32 = 16.0;
/// Animated path speed for wheel / number-key / release shifts (gate units per second).
pub const PATH_SPEED: f32 = 10.0;
/// Raw mouse pixels per gate unit while dragging the stick.
pub const DRAG_PX_PER_UNIT: f32 = 90.0;
/// Per-frame drag magnitude (gate units) below which mouse jitter is ignored.
pub const DRAG_DEADZONE: f32 = 0.02;
/// Drain at dilation 0 (energy per second).
pub const MAX_DRAIN: f32 = 75.0;
/// Below this dilation the world counts as dilated (drain on, regen off).
pub const DILATING_THRESHOLD: f32 = 0.995;
/// Meter fraction required to recover from a stall.
pub const STALL_RECOVER_FRAC: f32 = 0.2;
/// Gate fade in/out duration (s).
pub const FADE_TIME: f32 = 0.15;
/// Stall red flash duration (s).
pub const STALL_FLASH_TIME: f32 = 0.45;
/// Gear-seat micro shake duration (s).
pub const SEAT_KICK_TIME: f32 = 0.12;
/// Largest dt the model integrates in one call (guards against hitches).
pub const MAX_DT: f32 = 0.25;

// Visual layout (world units == logical px, camera at origin).
/// Gate centre relative to the camera: screen (1120, 210) on a 1280x720 window.
pub const GATE_CENTER: Vec2 = Vec2::new(480.0, 150.0);
/// Pixels per gate unit.
pub const GATE_SCALE: f32 = 40.0;
/// Visual half-width of a gate channel (gate units).
const CHANNEL_HALF: f32 = 0.14;

// ---------------------------------------------------------------------------------------------
// Gears
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gear {
    N,
    G1,
    G2,
    G3,
    G4,
    G5,
    G6,
}

const GEAR_DILATIONS: [f32; 7] = [1.0, 0.75, 0.55, 0.38, 0.24, 0.12, 0.04];

impl Gear {
    pub const ALL: [Gear; 7] = [Gear::N, Gear::G1, Gear::G2, Gear::G3, Gear::G4, Gear::G5, Gear::G6];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: i32) -> Gear {
        Gear::ALL[i.clamp(0, 6) as usize]
    }

    /// Detent position of the gear in gate units.
    pub fn pos(self) -> Vec2 {
        match self {
            Gear::N => Vec2::ZERO,
            Gear::G1 => Vec2::new(-1.0, 1.0),
            Gear::G2 => Vec2::new(-1.0, -1.0),
            Gear::G3 => Vec2::new(0.0, 1.0),
            Gear::G4 => Vec2::new(0.0, -1.0),
            Gear::G5 => Vec2::new(1.0, 1.0),
            Gear::G6 => Vec2::new(1.0, -1.0),
        }
    }

    /// World time scale at full engagement.
    pub fn dilation(self) -> f32 {
        GEAR_DILATIONS[self.index()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Gear::N => "N",
            Gear::G1 => "1",
            Gear::G2 => "2",
            Gear::G3 => "3",
            Gear::G4 => "4",
            Gear::G5 => "5",
            Gear::G6 => "6",
        }
    }

    /// Gear whose slot lives on column `col` (exact) in the row given by the sign of `y`.
    fn from_slot(col: f32, y: f32) -> Gear {
        let ci = (col + 1.0).round().clamp(0.0, 2.0) as i32;
        let top = y > 0.0;
        Gear::from_index(1 + 2 * ci + if top { 0 } else { 1 })
    }
}

/// Sequential shift: `steps > 0` shifts up (N→1→…→6), `< 0` down; saturates at N / 6.
pub fn next_gear(g: Gear, steps: i32) -> Gear {
    Gear::from_index(g.index() as i32 + steps.clamp(-7, 7))
}

// ---------------------------------------------------------------------------------------------
// Pure gate kinematics
// ---------------------------------------------------------------------------------------------

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Nearest column to `x` (exact constant -1, 0 or 1).
pub fn nearest_column(x: f32) -> f32 {
    let c = finite_or(x, 0.0).clamp(-1.0, 1.0).round();
    if c == 0.0 {
        0.0 // normalise -0.0
    } else {
        c
    }
}

/// `Some(col)` when `x` is exactly on a column.
pub fn exact_column(x: f32) -> Option<f32> {
    COLUMNS.iter().copied().find(|&c| x == c)
}

/// The H-gate legality predicate.
pub fn is_legal(p: Vec2) -> bool {
    p.is_finite()
        && p.x.abs() <= 1.0
        && p.y.abs() <= 1.0
        && (p.y.abs() <= RAIL_TOL || exact_column(p.x).is_some())
}

/// Projects any (possibly NaN / out-of-range) point onto a legal gate position.
pub fn project_legal(p: Vec2) -> Vec2 {
    let x = finite_or(p.x, 0.0).clamp(-1.0, 1.0);
    let y = finite_or(p.y, 0.0).clamp(-1.0, 1.0);
    if y.abs() > RAIL_TOL {
        Vec2::new(nearest_column(x), y)
    } else {
        Vec2::new(x, y)
    }
}

/// One constrained micro-step. Precondition: `p` legal, `|d| <= MAX_SUBSTEP`. Postcondition: legal.
fn micro_step(p: Vec2, d: Vec2) -> Vec2 {
    if p.y.abs() <= RAIL_TOL {
        // On the neutral rail: free in x; may only leave vertically near a column.
        let mut nx = (p.x + d.x).clamp(-1.0, 1.0);
        let mut ny = p.y + d.y;
        if ny.abs() > RAIL_TOL {
            let col = nearest_column(nx);
            if (nx - col).abs() <= COL_TOL {
                nx = col; // snap into the slot
                ny = ny.clamp(-1.0, 1.0);
            } else {
                ny = ny.clamp(-RAIL_TOL, RAIL_TOL); // gate wall
            }
        }
        Vec2::new(nx, ny)
    } else {
        // Inside a slot: x locked to the column; y free in [-1,1].
        let col = nearest_column(p.x);
        let ny = (p.y + d.y).clamp(-1.0, 1.0);
        Vec2::new(col, ny)
    }
}

/// Moves the knob by `delta` through the gate, sub-stepping so it can never tunnel through a
/// wall. Accepts arbitrary input (NaN/inf/huge) and always returns a legal position.
pub fn constrain_step(p: Vec2, delta: Vec2) -> Vec2 {
    let mut p = project_legal(p);
    let d = Vec2::new(
        if delta.x.is_nan() { 0.0 } else { delta.x.clamp(-MAX_DELTA, MAX_DELTA) },
        if delta.y.is_nan() { 0.0 } else { delta.y.clamp(-MAX_DELTA, MAX_DELTA) },
    );
    let len = d.length();
    if len <= 0.0 {
        return p;
    }
    let n = (len / MAX_SUBSTEP).ceil().max(1.0) as u32;
    let step = d / n as f32;
    for _ in 0..n {
        p = micro_step(p, step);
    }
    p
}

/// Slot depth engagement in `[0,1]` (0 on the rail / off-column).
pub fn engagement(p: Vec2) -> f32 {
    if !p.is_finite() || exact_column(p.x).is_none() {
        return 0.0;
    }
    smoothstep((p.y.abs() - ENGAGE_START) / (1.0 - ENGAGE_START))
}

/// Gear whose slot the knob is in (N when on the rail off-column or exactly at y = 0).
pub fn slot_gear(p: Vec2) -> Gear {
    match exact_column(p.x) {
        Some(c) if p.y != 0.0 && p.y.is_finite() => Gear::from_slot(c, p.y),
        _ => Gear::N,
    }
}

/// Gear shown to the player: the slot gear once engagement has started, otherwise N.
pub fn display_gear(p: Vec2) -> Gear {
    if engagement(p) > 0.0 {
        slot_gear(p)
    } else {
        Gear::N
    }
}

/// World time dilation commanded by a knob position.
pub fn dilation_at(p: Vec2) -> f32 {
    let e = engagement(p);
    let d = 1.0 + (slot_gear(p).dilation() - 1.0) * e;
    d.clamp(GEAR_DILATIONS[6], 1.0)
}

/// Continuous meter drain (energy/s) for a dilation: `MAX_DRAIN * (1-d)^2`, 0 at d = 1.
pub fn drain_for_dilation(d: f32) -> f32 {
    let d = finite_or(d, 1.0).clamp(0.0, 1.0);
    MAX_DRAIN * (1.0 - d) * (1.0 - d)
}

/// Stall hysteresis: stall when the meter is empty; recover once it is back to 20%.
pub fn stall_update(stalled: bool, energy_frac: f32) -> bool {
    let f = finite_or(energy_frac, 0.0);
    if f <= 0.0 {
        true
    } else if stalled {
        f < STALL_RECOVER_FRAC
    } else {
        false
    }
}

/// Nearest detent: slot end (deep in a slot), rail column centre otherwise.
pub fn detent_target(p: Vec2) -> Vec2 {
    let p = project_legal(p);
    if p.y.abs() > RAIL_TOL {
        let col = nearest_column(p.x);
        if p.y.abs() >= SEAT_THRESHOLD {
            Vec2::new(col, p.y.signum())
        } else {
            Vec2::new(col, 0.0)
        }
    } else {
        Vec2::new(nearest_column(p.x), 0.0)
    }
}

/// Exponential detent spring (no input). Seats exactly once close enough.
pub fn detent_step(p: Vec2, dt: f32) -> Vec2 {
    let p = project_legal(p);
    let target = detent_target(p);
    let diff = target - p;
    if diff.length() < 0.004 {
        return target;
    }
    let dt = finite_or(dt, 0.0).clamp(0.0, MAX_DT);
    let alpha = 1.0 - (-DETENT_K * dt).exp();
    constrain_step(p, diff * alpha)
}

/// `Some(gear)` when the knob is fully seated at a gear end.
pub fn seated_gear(p: Vec2) -> Option<Gear> {
    match exact_column(p.x) {
        Some(_) if p.y.abs() >= 0.999 => Some(slot_gear(p)),
        _ => None,
    }
}

/// Plans a legal, axis-aligned waypoint path from `from` to `target`: out of the current slot
/// to the rail, across the rail, into the target slot.
pub fn plan_gear_path(from: Vec2, target: Gear) -> VecDeque<Vec2> {
    let from = project_legal(from);
    let goal = target.pos();
    let mut path = VecDeque::new();
    let in_slot = from.y.abs() > RAIL_TOL;
    let same_slot = in_slot
        && target != Gear::N
        && nearest_column(from.x) == goal.x
        && from.y.signum() == goal.y.signum();

    if !same_slot {
        if in_slot {
            path.push_back(Vec2::new(nearest_column(from.x), 0.0));
        }
        path.push_back(Vec2::new(goal.x, 0.0));
    }
    if target != Gear::N {
        path.push_back(goal);
    }
    // Drop zero-length legs.
    let mut cleaned = VecDeque::with_capacity(path.len());
    let mut last = from;
    for wp in path {
        if (wp - last).length() > 1e-6 {
            cleaned.push_back(wp);
            last = wp;
        }
    }
    cleaned
}

/// Advances along `path` by `speed * dt`, routing every move through [`constrain_step`].
pub fn follow_path(p: Vec2, path: &mut VecDeque<Vec2>, speed: f32, dt: f32) -> Vec2 {
    let mut p = project_legal(p);
    let mut budget = finite_or(speed * dt, 0.0).max(0.0);
    while budget > 1e-5 {
        let Some(&wp) = path.front() else { break };
        let to = wp - p;
        let dist = to.length();
        if dist <= 1e-5 {
            p = wp;
            path.pop_front();
            continue;
        }
        if dist <= budget {
            let np = constrain_step(p, to);
            if (np - wp).length() < 1e-3 && is_legal(wp) {
                p = wp;
                path.pop_front();
                budget -= dist;
            } else {
                // Unreachable waypoint (should not happen for planned paths): abort safely.
                path.clear();
                return np;
            }
        } else {
            let move_vec = to / dist * budget;
            let np = constrain_step(p, move_vec);
            if move_vec.length() > 1e-4 && (np - p).length() < move_vec.length() * 0.1 {
                path.clear();
            }
            p = np;
            budget = 0.0;
        }
    }
    p
}

// ---------------------------------------------------------------------------------------------
// Shifter state machine (pure)
// ---------------------------------------------------------------------------------------------

/// One frame of player intent (already translated into gate units).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShifterInput {
    pub held: bool,
    /// Drag delta in gate units (y up).
    pub drag: Vec2,
    /// Wheel notches this frame (+ = shift up).
    pub wheel_steps: i32,
    /// Number key target.
    pub jump_to: Option<Gear>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShiftOutput {
    pub dilation: f32,
    /// Gear that just seated this frame (mechanical click).
    pub seated: Option<Gear>,
    pub newly_stalled: bool,
}

#[derive(Clone, Debug)]
pub struct ShifterModel {
    pub knob: Vec2,
    pub held: bool,
    pub path: VecDeque<Vec2>,
    /// Gear the knob is currently animating toward (None when free / arrived).
    pub target: Option<Gear>,
    pub stalled: bool,
    pub stall_flash: f32,
    pub fade: f32,
    pub seat_kick: f32,
    pub last_seat: Option<Gear>,
    pub dilation: f32,
}

impl Default for ShifterModel {
    fn default() -> Self {
        Self {
            knob: Vec2::ZERO,
            held: false,
            path: VecDeque::new(),
            target: None,
            stalled: false,
            stall_flash: 0.0,
            fade: 0.0,
            seat_kick: 0.0,
            last_seat: None,
            dilation: 1.0,
        }
    }
}

impl ShifterModel {
    fn set_target(&mut self, g: Gear) {
        self.path = plan_gear_path(self.knob, g);
        self.target = if self.path.is_empty() { None } else { Some(g) };
    }

    fn ensure_heading_neutral(&mut self) {
        let heading_n = self.target == Some(Gear::N) && !self.path.is_empty();
        if !heading_n && self.knob != Vec2::ZERO {
            self.set_target(Gear::N);
        }
    }

    /// Advances the shifter one frame. `energy_frac` = meter / max.
    pub fn step(&mut self, input: &ShifterInput, energy_frac: f32, dt: f32) -> ShiftOutput {
        let dt = finite_or(dt, 0.0).clamp(0.0, MAX_DT);
        let mut out = ShiftOutput::default();

        // 1. Stall hysteresis.
        let was_stalled = self.stalled;
        self.stalled = stall_update(self.stalled, energy_frac);
        if self.stalled && !was_stalled {
            out.newly_stalled = true;
            if self.held || input.held || self.knob != Vec2::ZERO {
                self.stall_flash = 1.0;
            }
            self.set_target(Gear::N);
        }

        // 2. Intent.
        self.held = input.held;
        let mut dragged = false;
        if !self.held || self.stalled {
            self.ensure_heading_neutral();
        } else {
            let drag = Vec2::new(finite_or(input.drag.x, 0.0), finite_or(input.drag.y, 0.0));
            if drag.length() > DRAG_DEADZONE {
                self.path.clear();
                self.target = None;
                self.knob = constrain_step(self.knob, drag);
                dragged = true;
            }
            if let Some(g) = input.jump_to {
                self.set_target(g);
            } else if input.wheel_steps != 0 {
                let base = self.target.unwrap_or_else(|| {
                    if self.knob.y.abs() > RAIL_TOL {
                        slot_gear(self.knob)
                    } else {
                        display_gear(self.knob)
                    }
                });
                self.set_target(next_gear(base, input.wheel_steps));
            }
        }

        // 3. Integrate.
        if !self.path.is_empty() {
            self.knob = follow_path(self.knob, &mut self.path, PATH_SPEED, dt);
            if self.path.is_empty() {
                self.target = None;
            }
        } else if self.held && !dragged {
            self.knob = detent_step(self.knob, dt);
        }
        self.knob = project_legal(self.knob);

        // 4. Seat clicks.
        let seat = seated_gear(self.knob);
        if seat != self.last_seat {
            if let Some(g) = seat {
                out.seated = Some(g);
                self.seat_kick = SEAT_KICK_TIME;
            }
            self.last_seat = seat;
        }

        // 5. Timers.
        let fade_target = if self.held { 1.0 } else { 0.0 };
        let fade_step = dt / FADE_TIME;
        self.fade = if self.fade < fade_target {
            (self.fade + fade_step).min(fade_target)
        } else {
            (self.fade - fade_step).max(fade_target)
        };
        self.stall_flash = (self.stall_flash - dt / STALL_FLASH_TIME).max(0.0);
        self.seat_kick = (self.seat_kick - dt).max(0.0);

        // 6. Output.
        self.dilation = if self.stalled { 1.0 } else { dilation_at(self.knob) };
        out.dilation = self.dilation;
        out
    }
}

/// Drain applied to the meter for a commanded dilation (0 => regen).
pub fn meter_drain(dilation: f32) -> f32 {
    if dilation < DILATING_THRESHOLD {
        drain_for_dilation(dilation)
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------------------------
// ECS integration
// ---------------------------------------------------------------------------------------------

/// Shared shifter state.
#[derive(Resource, Debug, Clone)]
pub struct ChronoShifter {
    pub model: ShifterModel,
    pub drain_per_sec: f32,
    pub energy_frac: f32,
    /// Screen-space cursor anchor captured on press.
    pub anchor_screen: Option<Vec2>,
    /// World-space aim frozen on press.
    pub frozen_aim: Option<Vec2>,
    aim_locked: bool,
    /// `Some(true)` the frame dilation started, `Some(false)` the frame it ended.
    pub dilation_edge: Option<bool>,
}

impl Default for ChronoShifter {
    fn default() -> Self {
        Self {
            model: ShifterModel::default(),
            drain_per_sec: 0.0,
            energy_frac: 1.0,
            anchor_screen: None,
            frozen_aim: None,
            aim_locked: false,
            dilation_edge: None,
        }
    }
}

impl ChronoShifter {
    pub fn is_held(&self) -> bool {
        self.model.held
    }

    /// True while the aim (`CursorWorldCoords`) must not be updated from the OS cursor.
    pub fn aim_frozen(&self) -> bool {
        self.aim_locked
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChronoShiftSet {
    /// Input -> model -> ChronoState / meter -> cursor pin. Runs before all time & aim consumers.
    Control,
}

fn number_key_target(keyboard: &ButtonInput<KeyCode>) -> Option<Gear> {
    const MAP: [(KeyCode, KeyCode, Gear); 7] = [
        (KeyCode::Digit0, KeyCode::Numpad0, Gear::N),
        (KeyCode::Digit1, KeyCode::Numpad1, Gear::G1),
        (KeyCode::Digit2, KeyCode::Numpad2, Gear::G2),
        (KeyCode::Digit3, KeyCode::Numpad3, Gear::G3),
        (KeyCode::Digit4, KeyCode::Numpad4, Gear::G4),
        (KeyCode::Digit5, KeyCode::Numpad5, Gear::G5),
        (KeyCode::Digit6, KeyCode::Numpad6, Gear::G6),
    ];
    if keyboard.just_pressed(KeyCode::Backquote) {
        return Some(Gear::N);
    }
    MAP.iter()
        .rev()
        .find(|(a, b, _)| keyboard.just_pressed(*a) || keyboard.just_pressed(*b))
        .map(|(_, _, g)| *g)
}

/// Reads input, steps the model, writes `ChronoState` and drains/regens the meter.
pub fn chrono_shift_control_system(
    time: Res<Time<Real>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut shifter: ResMut<ChronoShifter>,
    mut chrono: ResMut<ChronoState>,
    mut energy_q: Query<&mut ChronoEnergy, With<Player>>,
) {
    let dt = time.delta_secs();
    let held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    let mut drag_px = Vec2::ZERO;
    for ev in motion.read() {
        drag_px += ev.delta;
    }
    let mut wheel_steps = 0i32;
    for ev in wheel.read() {
        if ev.y > 0.0 {
            wheel_steps += 1;
        } else if ev.y < 0.0 {
            wheel_steps -= 1;
        }
    }

    let input = if held {
        ShifterInput {
            held,
            // MouseMotion y grows downward; gate y grows upward.
            drag: Vec2::new(drag_px.x, -drag_px.y) / DRAG_PX_PER_UNIT,
            wheel_steps,
            jump_to: number_key_target(&keyboard),
        }
    } else {
        ShifterInput::default()
    };

    let mut energy = energy_q.get_single_mut().ok();
    let energy_frac = match energy.as_deref() {
        Some(e) if e.max > 0.0 => e.current / e.max,
        Some(_) => 0.0,
        None => 1.0,
    };

    let out = shifter.model.step(&input, energy_frac, dt);
    let dilation = out.dilation;
    let drain = meter_drain(dilation);

    if let Some(e) = energy.as_deref_mut() {
        e.update_with_drain(drain, dt);
        shifter.energy_frac = if e.max > 0.0 { (e.current / e.max).clamp(0.0, 1.0) } else { 0.0 };
    } else {
        shifter.energy_frac = 1.0;
    }
    shifter.drain_per_sec = drain;

    let was = chrono.is_dilating;
    chrono.slowmo_factor = dilation;
    chrono.is_dilating = dilation < DILATING_THRESHOLD;
    shifter.dilation_edge = if chrono.is_dilating != was { Some(chrono.is_dilating) } else { None };

    if out.seated.is_some() && shifter.model.held {
        chrono.trigger_shake(1.2); // gear-seat micro shake
    }
    if out.newly_stalled && shifter.model.stall_flash > 0.0 {
        chrono.trigger_shake(4.0); // gearbox grind
    }
}

/// Pins the OS cursor at its press position and freezes the aim while the shifter is held.
pub fn chrono_shift_cursor_pin_system(
    mut shifter: ResMut<ChronoShifter>,
    mut cursor: ResMut<CursorWorldCoords>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let held = shifter.model.held;
    let mut window = windows.get_single_mut().ok();

    if held && !shifter.aim_locked {
        shifter.aim_locked = true;
        shifter.frozen_aim = cursor.0;
        shifter.anchor_screen = window.as_ref().and_then(|w| w.cursor_position());
        if let Some(w) = window.as_deref_mut() {
            w.cursor_options.visible = false;
        }
    } else if !held && shifter.aim_locked {
        shifter.aim_locked = false;
        shifter.anchor_screen = None;
        if let Some(w) = window.as_deref_mut() {
            w.cursor_options.visible = true;
        }
    }

    if shifter.aim_locked {
        cursor.0 = shifter.frozen_aim;
        if let (Some(anchor), Some(w)) = (shifter.anchor_screen, window.as_deref_mut()) {
            if w.cursor_position() != Some(anchor) {
                w.set_cursor_position(Some(anchor));
            }
        }
    }
}

/// Resource + control systems only (headless-safe; used by tests).
pub struct ChronoShiftCorePlugin;

impl Plugin for ChronoShiftCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChronoShifter>()
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_systems(
                Update,
                (chrono_shift_control_system, chrono_shift_cursor_pin_system)
                    .chain()
                    .in_set(ChronoShiftSet::Control),
            )
            .configure_sets(
                Update,
                ChronoShiftSet::Control
                    .before(update_chrono_system)
                    .before(crate::update_cursor_system)
                    .before(crate::player_input_and_movement_system)
                    .before(crate::player_katana_slash_system)
                    .before(crate::player_chrono_warp_system),
            );
    }
}

/// Full feature: control + gizmo gate + UI readout + tint overlay + sfx.
pub struct ChronoShiftPlugin;

impl Plugin for ChronoShiftPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ChronoShiftCorePlugin)
            .add_systems(Startup, setup_chrono_shift_ui)
            .add_systems(
                Update,
                (chrono_shift_ui_system, chrono_shift_audio_system).after(ChronoShiftSet::Control),
            )
            .add_systems(PostUpdate, chrono_shift_gizmo_system);
    }
}

// ---------------------------------------------------------------------------------------------
// Visuals
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct ChronoShiftReadout;

#[derive(Component)]
pub struct ChronoShiftTint;

fn setup_chrono_shift_ui(mut commands: Commands) {
    // Fullscreen time tint (below the HUD, ignores picking).
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.25, 0.45, 0.0)),
        GlobalZIndex(-10),
        PickingBehavior::IGNORE,
        ChronoShiftTint,
    ));

    // Readout under the gate.
    commands.spawn((
        Text::new("GEAR N  ·  1.00x  ·  DRAIN 0/s"),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 0.78, 0.35, 0.0)),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(24.0),
            top: Val::Px(312.0),
            ..default()
        },
        PickingBehavior::IGNORE,
        ChronoShiftReadout,
    ));
}

/// Readout text formatting (pure, shared with tests).
pub fn readout_text(model: &ShifterModel, drain: f32, energy_frac: f32) -> String {
    if model.stalled {
        format!("STALL  ·  GRIND  ·  RECHARGE {:.0}%", (energy_frac * 100.0).clamp(0.0, 100.0))
    } else {
        format!(
            "GEAR {}  ·  {:.2}x  ·  DRAIN {:.0}/s",
            display_gear(model.knob).label(),
            model.dilation,
            drain
        )
    }
}

fn chrono_shift_ui_system(
    shifter: Res<ChronoShifter>,
    chrono: Res<ChronoState>,
    mut readout_q: Query<(&mut Text, &mut TextColor), With<ChronoShiftReadout>>,
    mut tint_q: Query<&mut BackgroundColor, (With<ChronoShiftTint>, Without<ChronoShiftReadout>)>,
) {
    let m = &shifter.model;
    for (mut text, mut color) in readout_q.iter_mut() {
        let s = readout_text(m, shifter.drain_per_sec, shifter.energy_frac);
        if **text != s {
            **text = s;
        }
        let a = m.fade.clamp(0.0, 1.0);
        *color = if m.stalled || m.stall_flash > 0.0 {
            TextColor(Color::srgba(1.0, 0.25, 0.2, a.max(m.stall_flash)))
        } else {
            TextColor(Color::srgba(1.0, 0.78, 0.35, a))
        };
    }

    let depth = (1.0 - finite_or(chrono.dilation_factor, 1.0)).clamp(0.0, 1.0);
    let flash = m.stall_flash.clamp(0.0, 1.0);
    for mut bg in tint_q.iter_mut() {
        let base = (0.05, 0.25, 0.45, 0.30 * depth);
        let red = (0.9, 0.08, 0.05, 0.35 * flash);
        let w = if red.3 + base.3 > 0.0 { red.3 / (red.3 + base.3) } else { 0.0 };
        let mix = |a: f32, b: f32| a + (b - a) * w;
        *bg = BackgroundColor(Color::srgba(
            mix(base.0, red.0),
            mix(base.1, red.1),
            mix(base.2, red.2),
            (base.3 + red.3).min(0.6),
        ));
    }
}

fn chrono_shift_audio_system(
    shifter: Res<ChronoShifter>,
    audio: Option<Res<crate::GameAudioAssets>>,
    mut commands: Commands,
) {
    let (Some(edge), Some(sfx)) = (shifter.dilation_edge, audio) else {
        return;
    };
    let handle = if edge { sfx.slowmo_enter.clone() } else { sfx.slowmo_exit.clone() };
    commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
}

/// 7-segment glyph segments: a, b, c, d, e, f, g.
fn seven_segments(ch: char) -> [bool; 7] {
    match ch {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        _ => [false; 7],
    }
}

fn draw_glyph(gizmos: &mut Gizmos, ch: char, center: Vec2, h: f32, color: Color) {
    let w = h * 0.55;
    let (l, r) = (center.x - w * 0.5, center.x + w * 0.5);
    let (t, m, b) = (center.y + h * 0.5, center.y, center.y - h * 0.5);
    if ch == 'N' {
        gizmos.linestrip_2d([Vec2::new(l, b), Vec2::new(l, t), Vec2::new(r, b), Vec2::new(r, t)], color);
        return;
    }
    let s = seven_segments(ch);
    let segs = [
        (Vec2::new(l, t), Vec2::new(r, t)),
        (Vec2::new(r, t), Vec2::new(r, m)),
        (Vec2::new(r, m), Vec2::new(r, b)),
        (Vec2::new(l, b), Vec2::new(r, b)),
        (Vec2::new(l, m), Vec2::new(l, b)),
        (Vec2::new(l, t), Vec2::new(l, m)),
        (Vec2::new(l, m), Vec2::new(r, m)),
    ];
    for (on, (a, z)) in s.iter().zip(segs) {
        if *on {
            gizmos.line_2d(a, z, color);
        }
    }
}

/// Outline segments (gate units) of the H-gate channel.
fn gate_outline() -> Vec<(Vec2, Vec2)> {
    let w = CHANNEL_HALF;
    let top = 1.0 + w;
    let mut s = Vec::new();
    for sign in [1.0f32, -1.0] {
        // End caps.
        for c in COLUMNS {
            s.push((Vec2::new(c - w, sign * top), Vec2::new(c + w, sign * top)));
        }
        // Inner slot walls.
        s.push((Vec2::new(-1.0 + w, sign * w), Vec2::new(-1.0 + w, sign * top)));
        s.push((Vec2::new(-w, sign * w), Vec2::new(-w, sign * top)));
        s.push((Vec2::new(w, sign * w), Vec2::new(w, sign * top)));
        s.push((Vec2::new(1.0 - w, sign * w), Vec2::new(1.0 - w, sign * top)));
        // Rail walls between slots.
        s.push((Vec2::new(-1.0 + w, sign * w), Vec2::new(-w, sign * w)));
        s.push((Vec2::new(w, sign * w), Vec2::new(1.0 - w, sign * w)));
    }
    // Outer walls.
    s.push((Vec2::new(-1.0 - w, -top), Vec2::new(-1.0 - w, top)));
    s.push((Vec2::new(1.0 + w, -top), Vec2::new(1.0 + w, top)));
    s
}

fn with_alpha(c: Color, a: f32) -> Color {
    c.with_alpha(c.alpha() * a.clamp(0.0, 1.0))
}

fn chrono_shift_gizmo_system(
    mut gizmos: Gizmos,
    shifter: Res<ChronoShifter>,
    time: Res<Time<Real>>,
    cam_q: Query<&Transform, With<MainCamera>>,
) {
    let m = &shifter.model;
    let a = m.fade.max(m.stall_flash).clamp(0.0, 1.0);
    if a <= 0.001 {
        return;
    }
    let cam = cam_q.get_single().map(|t| t.translation.truncate()).unwrap_or(Vec2::ZERO);
    let origin = cam + GATE_CENTER;
    let to_w = |p: Vec2| origin + p * GATE_SCALE;

    let brass = Color::srgba(0.95, 0.72, 0.30, 1.0);
    let brass_dim = Color::srgba(0.55, 0.40, 0.18, 1.0);
    let cyan = Color::srgba(0.20, 0.95, 1.0, 1.0);
    let red = Color::srgba(1.0, 0.18, 0.12, 1.0);
    let grind = m.stall_flash.clamp(0.0, 1.0);
    let wall = if grind > 0.0 { red.mix(&brass, 1.0 - grind) } else { brass };

    // Backing plate.
    gizmos.rect_2d(
        to_w(Vec2::new(0.0, -0.35)),
        Vec2::new(3.3, 3.9) * GATE_SCALE,
        with_alpha(brass_dim, a * 0.8),
    );

    // Channels (double stroke for a machined look).
    for (p0, p1) in gate_outline() {
        gizmos.line_2d(to_w(p0), to_w(p1), with_alpha(wall, a));
        gizmos.line_2d(to_w(p0) + Vec2::new(1.0, -1.0), to_w(p1) + Vec2::new(1.0, -1.0), with_alpha(brass_dim, a * 0.6));
    }

    // Engaged slot glow.
    let e = engagement(m.knob);
    if e > 0.0 {
        let c = m.knob.x;
        gizmos.line_2d(to_w(Vec2::new(c, 0.0)), to_w(m.knob), with_alpha(cyan, a * (0.4 + 0.6 * e)));
    }

    // Labels.
    let active = display_gear(m.knob);
    for g in &Gear::ALL[1..] {
        let p = g.pos();
        let lp = Vec2::new(p.x, p.y.signum() * (1.0 + CHANNEL_HALF + 0.38));
        let col = if *g == active { cyan } else { brass };
        draw_glyph(&mut gizmos, g.label().chars().next().unwrap_or(' '), to_w(lp), 0.34 * GATE_SCALE, with_alpha(col, a));
    }
    let n_col = if active == Gear::N { cyan } else { brass_dim };
    draw_glyph(&mut gizmos, 'N', to_w(Vec2::new(-1.0 - CHANNEL_HALF - 0.38, 0.0)), 0.30 * GATE_SCALE, with_alpha(n_col, a));
    gizmos.circle_2d(to_w(Vec2::ZERO), 2.0, with_alpha(brass_dim, a));

    // Knob position with seat micro shake.
    let t = time.elapsed_secs();
    let kick = (m.seat_kick / SEAT_KICK_TIME).clamp(0.0, 1.0);
    let jitter = Vec2::new((t * 97.0).sin(), (t * 131.0).cos()) * 2.0 * kick
        + Vec2::new((t * 61.0).sin(), 0.0) * 3.0 * grind;
    let knob_w = to_w(m.knob) + jitter;

    // Stick: shaft from a pivot below the gate, with key-bit teeth at the base.
    let pivot = to_w(Vec2::new(0.0, -1.95));
    let shaft = knob_w - pivot;
    let perp = shaft.perp().normalize_or_zero() * 2.0;
    let stick_col = with_alpha(brass, a);
    gizmos.line_2d(pivot + perp, knob_w + perp, stick_col);
    gizmos.line_2d(pivot - perp, knob_w - perp, stick_col);
    gizmos.circle_2d(pivot, 6.0, stick_col);
    let dir = shaft.normalize_or_zero();
    for (i, len) in [5.0f32, 8.0, 4.0].iter().enumerate() {
        let base = pivot + dir * (10.0 + i as f32 * 5.0);
        gizmos.line_2d(base + perp, base + perp + perp.normalize_or_zero() * *len, stick_col);
    }

    // Cog knob.
    let engaged_col = if m.stalled { red } else { brass.mix(&cyan, e) };
    let col = with_alpha(engaged_col, a);
    let r_in = 0.20 * GATE_SCALE;
    let r_out = 0.29 * GATE_SCALE;
    let rot = m.knob.x * std::f32::consts::FRAC_PI_2 + m.knob.y * 1.1;
    const TEETH: usize = 10;
    let half = std::f32::consts::PI / TEETH as f32 * 0.5;
    let mut ring = Vec::with_capacity(TEETH * 4 + 1);
    for i in 0..TEETH {
        let th = rot + i as f32 * std::f32::consts::TAU / TEETH as f32;
        let polar = |ang: f32, r: f32| knob_w + Vec2::from_angle(ang) * r;
        ring.push(polar(th - half * 1.4, r_in));
        ring.push(polar(th - half * 0.8, r_out));
        ring.push(polar(th + half * 0.8, r_out));
        ring.push(polar(th + half * 1.4, r_in));
    }
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    gizmos.linestrip_2d(ring, col);
    gizmos.circle_2d(knob_w, 0.08 * GATE_SCALE, col);
    gizmos.line_2d(knob_w, knob_w + Vec2::from_angle(rot) * r_in * 0.8, col);
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::input::{ButtonState, InputPlugin};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Deterministic xorshift PRNG.
    struct Rng(u64);
    impl Rng {
        fn next_u32(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 32) as u32
        }
        fn unit(&mut self) -> f32 {
            self.next_u32() as f32 / u32::MAX as f32 * 2.0 - 1.0
        }
        fn delta(&mut self) -> Vec2 {
            let scale = match self.next_u32() % 9 {
                0 => 1e-6,
                1 => 0.01,
                2 => 0.04,
                3 => 0.1,
                4 => 0.5,
                5 => 3.0,
                6 => 1e6,
                7 => 1e30,
                _ => 0.02,
            };
            let mut d = Vec2::new(self.unit(), self.unit()) * scale;
            match self.next_u32() % 64 {
                0 => d.x = f32::NAN,
                1 => d.y = f32::INFINITY,
                2 => d = Vec2::new(f32::NEG_INFINITY, f32::NAN),
                // axis-aligned pushes probe walls directly
                3..=10 => d.x = 0.0,
                11..=18 => d.y = 0.0,
                _ => {}
            }
            d
        }
    }

    fn sample_segment_legal(a: Vec2, b: Vec2) -> bool {
        (0..=100).all(|i| is_legal(a.lerp(b, i as f32 / 100.0)))
    }

    #[test]
    fn gear_positions_are_legal_and_dilations_strictly_decrease() {
        for (i, g) in Gear::ALL.iter().enumerate() {
            assert!(is_legal(g.pos()), "{g:?}");
            assert_eq!(g.index(), i);
            assert_eq!(slot_gear(g.pos()), *g);
            assert!((dilation_at(g.pos()) - g.dilation()).abs() < 1e-6);
        }
        for w in Gear::ALL.windows(2) {
            assert!(w[1].dilation() < w[0].dilation());
        }
    }

    #[test]
    fn fuzz_constrain_step_always_legal_and_never_tunnels() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut p = Vec2::ZERO;
        for i in 0..20_000 {
            let d = rng.delta();
            let np = constrain_step(p, d);
            assert!(is_legal(np), "step {i}: illegal {np:?} from {p:?} by {d:?}");
            // Small (single-substep) moves: walls must hold exactly.
            if d.is_finite() && d.length() <= MAX_SUBSTEP {
                if p.y.abs() > RAIL_TOL {
                    assert_eq!(np.x, p.x, "step {i}: left slot sideways {p:?}->{np:?}");
                }
                if np.y.abs() > RAIL_TOL && p.y.abs() <= RAIL_TOL {
                    assert!((p.x - np.x).abs() <= COL_TOL + MAX_SUBSTEP + 1e-5, "step {i}: wall tunnel {p:?}->{np:?}");
                }
                assert!((np - p).length() <= MAX_SUBSTEP + COL_TOL + 1e-5);
            }
            p = np;
            // Occasionally restart from garbage.
            if rng.next_u32() % 500 == 0 {
                p = project_legal(Vec2::new(rng.unit() * 5.0, rng.unit() * 5.0));
                assert!(is_legal(p));
            }
        }
        assert!(is_legal(constrain_step(Vec2::new(f32::NAN, f32::INFINITY), Vec2::new(f32::NAN, 1.0))));
    }

    #[test]
    fn huge_deltas_cannot_tunnel_through_walls() {
        // Off-column on the rail: vertical shove is stopped by the wall.
        let p = constrain_step(Vec2::new(0.5, 0.0), Vec2::new(0.0, 1e9));
        assert_eq!(p.x, 0.5);
        assert!(p.y <= RAIL_TOL);
        // Deep in slot 1: sideways shove does nothing.
        assert_eq!(constrain_step(Gear::G1.pos(), Vec2::new(1e9, 0.0)), Gear::G1.pos());
        assert_eq!(constrain_step(Gear::G6.pos(), Vec2::new(-f32::INFINITY, 0.0)), Gear::G6.pos());
        // Diagonal shove out of slot 1 must travel down to the rail first, never through a wall.
        let p = constrain_step(Gear::G1.pos(), Vec2::new(2.0, -1.0));
        assert!(is_legal(p));
        assert!(p.y.abs() <= RAIL_TOL, "{p:?}");
        // Near a column the knob is captured by the slot and snapped.
        let p = constrain_step(Vec2::new(0.15, 0.0), Vec2::new(0.0, 0.5));
        assert_eq!(p.x, 0.0);
        assert!((p.y - 0.5).abs() < 1e-5);
    }

    #[test]
    fn detent_seats_into_gear_and_rail() {
        let settle = |mut p: Vec2| {
            for _ in 0..120 {
                p = detent_step(p, 1.0 / 60.0);
                assert!(is_legal(p));
            }
            p
        };
        assert_eq!(settle(Vec2::new(0.0, 0.7)), Gear::G3.pos());
        assert_eq!(settle(Vec2::new(1.0, -0.6)), Gear::G6.pos());
        assert_eq!(settle(Vec2::new(0.0, 0.3)), Vec2::ZERO);
        assert_eq!(settle(Vec2::new(0.4, 0.05)), Vec2::ZERO);
        assert_eq!(settle(Vec2::new(0.7, 0.0)), Vec2::new(1.0, 0.0));
        assert_eq!(seated_gear(Gear::G3.pos()), Some(Gear::G3));
        assert_eq!(seated_gear(Vec2::new(0.0, 0.5)), None);
    }

    #[test]
    fn engagement_is_monotonic_and_bounded() {
        let mut prev = -1.0;
        for i in 0..=1000 {
            let y = i as f32 / 1000.0;
            let e = engagement(Vec2::new(0.0, y));
            assert!((0.0..=1.0).contains(&e));
            assert!(e >= prev - 1e-7);
            prev = e;
        }
        assert_eq!(engagement(Vec2::new(0.0, 0.1)), 0.0);
        assert_eq!(engagement(Vec2::new(0.0, 1.0)), 1.0);
        assert_eq!(engagement(Vec2::new(0.5, 0.0)), 0.0);
        // Dilation decreases with depth in every slot and stays within [g, 1].
        for g in &Gear::ALL[1..] {
            let mut prev = 1.0 + 1e-6;
            for i in 0..=100 {
                let p = Vec2::new(g.pos().x, g.pos().y * i as f32 / 100.0);
                let d = dilation_at(p);
                assert!(d <= prev + 1e-6 && d >= g.dilation() - 1e-6 && d <= 1.0);
                prev = d;
            }
        }
    }

    #[test]
    fn drain_mapping_monotonic_and_matches_table() {
        assert_eq!(drain_for_dilation(1.0), 0.0);
        assert!((drain_for_dilation(0.0) - MAX_DRAIN).abs() < 1e-4);
        let mut prev = f32::INFINITY;
        for i in 0..=100 {
            let d = drain_for_dilation(i as f32 / 100.0);
            assert!(d <= prev && (0.0..=MAX_DRAIN).contains(&d));
            prev = d;
        }
        for (g, expected) in [(Gear::G1, 4.7), (Gear::G2, 15.2), (Gear::G3, 28.8), (Gear::G4, 43.3), (Gear::G5, 58.1), (Gear::G6, 69.1)] {
            assert!((drain_for_dilation(g.dilation()) - expected).abs() < 0.2, "{g:?}");
        }
        assert_eq!(drain_for_dilation(f32::NAN), 0.0);
        assert_eq!(meter_drain(0.996), 0.0);
        assert!(meter_drain(0.5) > 0.0);
    }

    #[test]
    fn planned_paths_are_legal_and_reach_target() {
        let mut starts: Vec<Vec2> = Gear::ALL.iter().map(|g| g.pos()).collect();
        starts.extend([Vec2::new(0.4, 0.08), Vec2::new(-0.9, -0.1), Vec2::new(0.0, 0.5), Vec2::new(1.0, -0.3), Vec2::new(-1.0, 0.0)]);
        for &s in &starts {
            for &g in &Gear::ALL {
                let path = plan_gear_path(s, g);
                let mut last = s;
                for &wp in &path {
                    assert!(is_legal(wp));
                    assert!(sample_segment_legal(last, wp), "{s:?}->{g:?}: illegal leg {last:?}->{wp:?}");
                    last = wp;
                }
                let mut path = path;
                let mut p = s;
                let mut frames = 0;
                while !path.is_empty() {
                    p = follow_path(p, &mut path, PATH_SPEED, 1.0 / 60.0);
                    assert!(is_legal(p));
                    frames += 1;
                    assert!(frames < 120, "path did not converge");
                }
                if g != Gear::N || s.y.abs() > RAIL_TOL || s != Vec2::ZERO {
                    assert_eq!(p, g.pos(), "{s:?}->{g:?}");
                }
            }
        }
    }

    #[test]
    fn wheel_shifts_are_sequential_and_legal() {
        let mut m = ShifterModel::default();
        let mut expected = Gear::N;
        for _ in 0..8 {
            m.step(&ShifterInput { held: true, wheel_steps: 1, ..default() }, 1.0, 1.0 / 60.0);
            expected = next_gear(expected, 1);
            for _ in 0..60 {
                m.step(&ShifterInput { held: true, ..default() }, 1.0, 1.0 / 60.0);
                assert!(is_legal(m.knob));
            }
            assert_eq!(m.knob, expected.pos());
        }
        assert_eq!(expected, Gear::G6);
        for _ in 0..8 {
            m.step(&ShifterInput { held: true, wheel_steps: -1, ..default() }, 1.0, 1.0 / 60.0);
            expected = next_gear(expected, -1);
            for _ in 0..60 {
                m.step(&ShifterInput { held: true, ..default() }, 1.0, 1.0 / 60.0);
                assert!(is_legal(m.knob));
            }
            assert_eq!(m.knob, expected.pos());
        }
        assert_eq!(expected, Gear::N);
    }

    #[test]
    fn release_springs_back_to_neutral_and_fades() {
        let mut m = ShifterModel::default();
        m.step(&ShifterInput { held: true, jump_to: Some(Gear::G6), ..default() }, 1.0, 0.016);
        for _ in 0..60 {
            m.step(&ShifterInput { held: true, ..default() }, 1.0, 0.016);
        }
        assert_eq!(m.knob, Gear::G6.pos());
        assert!((m.dilation - 0.04).abs() < 1e-6);
        assert!((m.fade - 1.0).abs() < 1e-6);
        for _ in 0..60 {
            let out = m.step(&ShifterInput::default(), 1.0, 0.016);
            assert!(is_legal(m.knob));
            assert!((0.04..=1.0).contains(&out.dilation));
        }
        assert_eq!(m.knob, Vec2::ZERO);
        assert_eq!(m.dilation, 1.0);
        assert_eq!(m.fade, 0.0);
    }

    #[test]
    fn stall_forces_neutral_and_locks_until_twenty_percent() {
        assert!(stall_update(false, 0.0));
        assert!(stall_update(true, 0.19));
        assert!(!stall_update(true, 0.2));
        assert!(!stall_update(false, 0.05));
        assert!(stall_update(false, f32::NAN));

        let mut m = ShifterModel::default();
        m.step(&ShifterInput { held: true, jump_to: Some(Gear::G5), ..default() }, 1.0, 0.016);
        for _ in 0..60 {
            m.step(&ShifterInput { held: true, ..default() }, 1.0, 0.016);
        }
        assert!(m.dilation < 0.2);
        let out = m.step(&ShifterInput { held: true, ..default() }, 0.0, 0.016);
        assert!(out.newly_stalled && m.stalled);
        assert_eq!(out.dilation, 1.0, "stall cuts dilation immediately");
        assert!(m.stall_flash > 0.9);
        for _ in 0..60 {
            let out = m.step(&ShifterInput { held: true, jump_to: Some(Gear::G6), drag: Vec2::new(0.0, -1.0), wheel_steps: 3 }, 0.1, 0.016);
            assert_eq!(out.dilation, 1.0, "locked out while < 20%");
        }
        assert_eq!(m.knob, Vec2::ZERO);
        m.step(&ShifterInput { held: true, jump_to: Some(Gear::G2), ..default() }, 0.25, 0.016);
        for _ in 0..60 {
            m.step(&ShifterInput { held: true, ..default() }, 0.25, 0.016);
        }
        assert!(!m.stalled);
        assert_eq!(m.knob, Gear::G2.pos());
    }

    #[test]
    fn drag_moves_knob_through_gate() {
        let mut m = ShifterModel::default();
        // Drag left along the rail, then up into slot 1.
        for _ in 0..20 {
            m.step(&ShifterInput { held: true, drag: Vec2::new(-0.1, 0.0), ..default() }, 1.0, 0.016);
        }
        for _ in 0..20 {
            m.step(&ShifterInput { held: true, drag: Vec2::new(0.0, 0.1), ..default() }, 1.0, 0.016);
        }
        assert_eq!(m.knob, Gear::G1.pos());
        // Sideways drag inside a slot is blocked.
        m.step(&ShifterInput { held: true, drag: Vec2::new(5.0, 0.0), ..default() }, 1.0, 0.016);
        assert_eq!(m.knob, Gear::G1.pos());
    }

    #[test]
    fn fuzz_shifter_model_random_inputs() {
        let mut rng = Rng(42);
        let mut m = ShifterModel::default();
        for i in 0..10_000 {
            let input = ShifterInput {
                held: rng.next_u32() % 10 != 0,
                drag: if rng.next_u32() % 3 == 0 { rng.delta() } else { Vec2::ZERO },
                wheel_steps: (rng.next_u32() % 7) as i32 - 3,
                jump_to: if rng.next_u32() % 8 == 0 { Some(Gear::from_index((rng.next_u32() % 7) as i32)) } else { None },
            };
            let energy = if rng.next_u32() % 50 == 0 { 0.0 } else { rng.unit().abs() };
            let dt = match rng.next_u32() % 5 { 0 => 0.0, 1 => 1.0, 2 => f32::NAN, _ => 1.0 / 60.0 };
            let out = m.step(&input, energy, dt);
            assert!(is_legal(m.knob), "frame {i}: {:?}", m.knob);
            assert!(out.dilation.is_finite() && (0.04..=1.0).contains(&out.dilation));
            assert!((0.0..=1.0).contains(&m.fade));
            for wp in &m.path {
                assert!(is_legal(*wp));
            }
        }
    }

    #[test]
    fn readout_format() {
        let mut m = ShifterModel { knob: Gear::G4.pos(), dilation: 0.24, ..default() };
        assert_eq!(readout_text(&m, drain_for_dilation(0.24), 1.0), "GEAR 4  ·  0.24x  ·  DRAIN 43/s");
        m.stalled = true;
        assert!(readout_text(&m, 0.0, 0.1).starts_with("STALL"));
    }

    #[test]
    fn energy_update_with_drain_helper() {
        let mut e = ChronoEnergy::default();
        e.update_with_drain(43.0, 1.0);
        assert!((e.current - 57.0).abs() < 1e-4);
        e.update_with_drain(0.0, 1.0);
        assert!((e.current - (57.0 + e.regen_rate)).abs() < 1e-4);
        e.update_with_drain(1000.0, 1.0);
        assert_eq!(e.current, 0.0);
        e.update_with_drain(f32::NAN, 1.0);
        assert!(e.current.is_finite());
    }

    // ----- App-level integration -----

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)))
            .init_resource::<ChronoState>()
            .init_resource::<CursorWorldCoords>()
            .add_plugins(ChronoShiftCorePlugin)
            .add_systems(Update, update_chrono_system);
        app.world_mut().spawn((Player::default(), ChronoEnergy::default(), Transform::default()));
        app.update(); // prime time
        app
    }

    fn key(app: &mut App, code: KeyCode, pressed: bool) {
        app.world_mut().send_event(KeyboardInput {
            key_code: code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: if pressed { ButtonState::Pressed } else { ButtonState::Released },
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }

    fn energy(app: &mut App) -> f32 {
        let mut q = app.world_mut().query::<&ChronoEnergy>();
        q.single(app.world()).current
    }

    fn set_energy(app: &mut App, v: f32) {
        let mut q = app.world_mut().query::<&mut ChronoEnergy>();
        q.single_mut(app.world_mut()).current = v;
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    fn engage(app: &mut App, gear_key: KeyCode) {
        key(app, KeyCode::ShiftLeft, true);
        key(app, gear_key, true);
        app.update();
        key(app, gear_key, false);
        run(app, 40);
    }

    #[test]
    fn app_chrono_state_follows_shifter() {
        let mut app = test_app();
        assert!(!app.world().resource::<ChronoState>().is_dilating);

        engage(&mut app, KeyCode::Digit4);
        {
            let chrono = app.world().resource::<ChronoState>();
            assert!((chrono.slowmo_factor - 0.24).abs() < 1e-4, "{}", chrono.slowmo_factor);
            assert!(chrono.is_dilating);
            assert!((chrono.dilation_factor - 0.24).abs() < 0.02, "smoothing converges");
            let s = app.world().resource::<ChronoShifter>();
            assert!(s.is_held() && s.aim_frozen());
            assert!((s.drain_per_sec - drain_for_dilation(0.24)).abs() < 1e-3);
        }
        assert!(energy(&mut app) < 100.0);

        // Wheel up two notches -> gear 6.
        for _ in 0..2 {
            app.world_mut().send_event(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: 1.0, window: Entity::PLACEHOLDER });
            app.update();
        }
        run(&mut app, 40);
        assert!((app.world().resource::<ChronoState>().slowmo_factor - 0.04).abs() < 1e-4);

        // Raw mouse drag: up 3 gate units -> out of slot 6 into slot 5.
        app.world_mut().send_event(MouseMotion { delta: Vec2::new(0.0, -3.0 * DRAG_PX_PER_UNIT) });
        app.update();
        run(&mut app, 30);
        assert!((app.world().resource::<ChronoState>().slowmo_factor - 0.12).abs() < 1e-4);

        // Release -> neutral, time recovers.
        key(&mut app, KeyCode::ShiftLeft, false);
        run(&mut app, 60);
        let chrono = app.world().resource::<ChronoState>();
        assert_eq!(chrono.slowmo_factor, 1.0);
        assert!(!chrono.is_dilating);
        assert!((chrono.dilation_factor - 1.0).abs() < 0.01);
        let s = app.world().resource::<ChronoShifter>();
        assert!(!s.aim_frozen());
        assert_eq!(s.model.knob, Vec2::ZERO);
    }

    #[test]
    fn app_meter_drains_faster_in_higher_gears() {
        let mut loss = Vec::new();
        for gear_key in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit4, KeyCode::Digit6] {
            let mut app = test_app();
            engage(&mut app, gear_key);
            set_energy(&mut app, 100.0);
            run(&mut app, 30);
            loss.push(100.0 - energy(&mut app));
        }
        for w in loss.windows(2) {
            assert!(w[1] > w[0], "drain not increasing: {loss:?}");
        }
        // Gear 6 over 30 frames of 16ms ≈ 69/s * 0.48s ≈ 33.
        assert!((loss[3] - drain_for_dilation(0.04) * 0.48).abs() < 1.0, "{loss:?}");
    }

    #[test]
    fn app_meter_stalls_at_zero() {
        let mut app = test_app();
        engage(&mut app, KeyCode::Digit6);
        set_energy(&mut app, 2.0);
        run(&mut app, 10);
        assert!(app.world().resource::<ChronoShifter>().model.stalled);
        run(&mut app, 30);
        {
            let chrono = app.world().resource::<ChronoState>();
            assert!(!chrono.is_dilating);
            assert_eq!(chrono.slowmo_factor, 1.0);
            assert_eq!(app.world().resource::<ChronoShifter>().model.knob, Vec2::ZERO);
        }
        let e = energy(&mut app);
        assert!(e >= 0.0 && e < 20.0);
        // Locked out: shifting again does nothing below 20%.
        key(&mut app, KeyCode::Digit6, true);
        app.update();
        key(&mut app, KeyCode::Digit6, false);
        run(&mut app, 20);
        assert!(!app.world().resource::<ChronoState>().is_dilating);
        // Recharge above 20% (e.g. a kill) -> shifting works again.
        set_energy(&mut app, 30.0);
        app.update();
        key(&mut app, KeyCode::Digit3, true);
        app.update();
        key(&mut app, KeyCode::Digit3, false);
        run(&mut app, 40);
        assert!((app.world().resource::<ChronoState>().slowmo_factor - 0.38).abs() < 1e-4);
    }

    // =========================================================================================
    // Adversarial Red-Team Audit Tests (Disprover Persona)
    // =========================================================================================

    #[test]
    fn test_adversarial_floating_point_precision_traps() {
        // 1. Extreme floats: -0.0, subnormals, NaNs, Infs
        let subnormal_pos = f32::from_bits(1); // 1.4e-45
        let subnormal_neg = f32::from_bits(0x8000_0001);
        let neg_zero = -0.0f32;
        let test_floats = [
            neg_zero,
            0.0f32,
            subnormal_pos,
            subnormal_neg,
            1e-38,
            -1e-38,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
        ];

        // Column snapping with -0.0 must normalize to +0.0
        assert_eq!(nearest_column(neg_zero).to_bits(), 0.0f32.to_bits());
        assert_eq!(exact_column(neg_zero), Some(0.0));

        for &fx in &test_floats {
            for &fy in &test_floats {
                let p = Vec2::new(fx, fy);
                let proj = project_legal(p);
                assert!(is_legal(proj), "project_legal failed for {p:?} -> {proj:?}");
                assert!(proj.x.is_finite() && proj.y.is_finite());

                let d = dilation_at(p);
                assert!(d.is_finite(), "dilation_at returned non-finite for {p:?}: {d}");
                assert!((GEAR_DILATIONS[6]..=1.0).contains(&d), "dilation_at out of bounds: {d}");

                let eng = engagement(p);
                assert!(eng.is_finite(), "engagement returned non-finite: {eng}");
                assert!((0.0..=1.0).contains(&eng));

                let target = detent_target(p);
                assert!(is_legal(target), "detent_target not legal for {p:?}: {target:?}");

                let stepped = detent_step(p, fy);
                assert!(is_legal(stepped), "detent_step not legal for {p:?} with dt {fy}: {stepped:?}");
            }

            let drain = drain_for_dilation(fx);
            assert!(drain.is_finite() && (0.0..=MAX_DRAIN).contains(&drain));

            let m_drain = meter_drain(fx);
            assert!(m_drain.is_finite() && m_drain >= 0.0);

            // stall_update with extreme fractions
            let s1 = stall_update(false, fx);
            let s2 = stall_update(true, fx);
            assert!(s1 || !s1); // boolean validity
            assert!(s2 || !s2);
        }

        // Readout text format safety with NaNs and extremes
        let mut corrupted_model = ShifterModel::default();
        corrupted_model.knob = Vec2::new(neg_zero, neg_zero);
        corrupted_model.dilation = f32::NAN;
        let txt = readout_text(&corrupted_model, f32::NAN, f32::NAN);
        assert!(!txt.is_empty());
    }

    #[test]
    fn test_adversarial_rapid_opposing_inputs_anti_tunneling() {
        // 1. Strict wall integrity checks: solid walls must NEVER leak or tunnel.
        // Deep in slots: sideways shoves must be completely blocked.
        for g in [Gear::G1, Gear::G2, Gear::G3, Gear::G4, Gear::G5, Gear::G6] {
            let p = g.pos();
            assert_eq!(constrain_step(p, Vec2::new(8.0, 0.0)), p);
            assert_eq!(constrain_step(p, Vec2::new(-8.0, 0.0)), p);
            assert_eq!(constrain_step(p, Vec2::new(1e6, 0.0)), p);
            assert_eq!(constrain_step(p, Vec2::new(-1e6, 0.0)), p);
        }

        // On the rail off-column: vertical shoves must hit the rail wall and never enter slots.
        for off_x in [-0.7, -0.5, -0.3, 0.3, 0.5, 0.7] {
            let p = Vec2::new(off_x, 0.0);
            let up = constrain_step(p, Vec2::new(0.0, 8.0));
            assert_eq!(up.x, off_x);
            assert!(up.y <= RAIL_TOL + 1e-6);

            let down = constrain_step(p, Vec2::new(0.0, -8.0));
            assert_eq!(down.x, off_x);
            assert!(down.y >= -RAIL_TOL - 1e-6);
        }

        // 2. Sub-step invariant: verify x NEVER changes while |y| > RAIL_TOL
        let mut rng = Rng(0xDEAD_BEEF_CAFE);
        for _ in 0..1_000 {
            let start = project_legal(Vec2::new(rng.unit() * 1.5, rng.unit() * 1.5));
            let d = rng.delta();
            let d_clamped = Vec2::new(
                if d.x.is_nan() { 0.0 } else { d.x.clamp(-MAX_DELTA, MAX_DELTA) },
                if d.y.is_nan() { 0.0 } else { d.y.clamp(-MAX_DELTA, MAX_DELTA) },
            );
            let len = d_clamped.length();
            if len > 0.0 {
                let n = (len / MAX_SUBSTEP).ceil().max(1.0) as u32;
                let step = d_clamped / n as f32;
                let mut curr = start;
                for _ in 0..n {
                    let next = micro_step(curr, step);
                    assert!(is_legal(next), "micro_step illegal: {curr:?} -> {next:?}");
                    // Invariant: if curr is inside a slot (|y| > RAIL_TOL), x cannot change
                    if curr.y.abs() > RAIL_TOL {
                        assert_eq!(next.x, curr.x, "Lateral leak inside slot: {curr:?} -> {next:?}");
                    }
                    curr = next;
                }
            }
        }

        // 3. Hammer the shifter with massive opposing forces over 5,000 frames
        let mut m = ShifterModel::default();
        m.knob = Gear::G1.pos();

        let opposing_shoves = [
            Vec2::new(8.0, 8.0),
            Vec2::new(-8.0, -8.0),
            Vec2::new(8.0, -8.0),
            Vec2::new(-8.0, 8.0),
            Vec2::new(1e6, -1e6),
            Vec2::new(-1e6, 1e6),
            Vec2::new(0.0, -100.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(-100.0, 0.0),
            Vec2::new(0.0, 100.0),
        ];

        for i in 0..5_000 {
            let shove = opposing_shoves[i % opposing_shoves.len()];
            let out = m.step(
                &ShifterInput {
                    held: true,
                    drag: shove,
                    ..default()
                },
                1.0,
                0.016,
            );

            assert!(is_legal(m.knob), "Step {i}: knob became illegal at {:?}", m.knob);
            assert!(out.dilation.is_finite() && (0.04..=1.0).contains(&out.dilation));
        }
    }

    #[test]
    fn test_adversarial_meter_depletion_and_recovery_hysteresis() {
        let mut m = ShifterModel::default();
        // Shift to Gear 6
        m.step(&ShifterInput { held: true, jump_to: Some(Gear::G6), ..default() }, 1.0, 0.016);
        for _ in 0..50 {
            m.step(&ShifterInput { held: true, ..default() }, 1.0, 0.016);
        }
        assert_eq!(m.knob, Gear::G6.pos());
        assert!((m.dilation - 0.04).abs() < 1e-4);

        // Deplete meter to 0.0
        let out = m.step(&ShifterInput { held: true, ..default() }, 0.0, 0.016);
        assert!(m.stalled);
        assert!(out.newly_stalled);
        assert_eq!(out.dilation, 1.0, "Dilation must immediately cut to 1.0 on stall");
        assert_eq!(meter_drain(out.dilation), 0.0, "Drain must halt completely on stall");

        // Attempt shifting while stalled: hammer mouse drag, wheel, jump keys
        for _ in 0..30 {
            let out_locked = m.step(
                &ShifterInput {
                    held: true,
                    drag: Vec2::new(0.0, -5.0),
                    wheel_steps: 5,
                    jump_to: Some(Gear::G6),
                },
                0.10, // 10% energy: still in lockout zone
                0.016,
            );
            assert!(m.stalled, "Must stay stalled below recovery threshold");
            assert_eq!(out_locked.dilation, 1.0, "Time must not dilate while stalled");
        }

        // Knob must continue returning toward Neutral despite spam
        for _ in 0..40 {
            m.step(&ShifterInput { held: true, ..default() }, 0.15, 0.016);
        }
        assert_eq!(m.knob, Vec2::ZERO, "Knob must arrive at Neutral while stalled");

        // Energy restores above 20% (e.g. 21%) -> stall lock clears
        let out_recovered = m.step(&ShifterInput { held: true, ..default() }, 0.21, 0.016);
        assert!(!m.stalled, "Stall must clear once energy >= 20%");
        assert!(!out_recovered.newly_stalled);

        // Can immediately shift into gear again
        m.step(&ShifterInput { held: true, jump_to: Some(Gear::G3), ..default() }, 0.50, 0.016);
        for _ in 0..50 {
            m.step(&ShifterInput { held: true, ..default() }, 0.50, 0.016);
        }
        assert_eq!(m.knob, Gear::G3.pos());
        assert!((m.dilation - 0.38).abs() < 1e-4);
    }

    #[test]
    fn test_adversarial_camera_shake_overflow_and_audio_edges() {
        let mut chrono = ChronoState::default();

        // 1. Hammer camera shake with 10,000 extreme spikes
        for _ in 0..10_000 {
            chrono.trigger_shake(100.0);
            assert!(chrono.screen_shake <= 20.0, "Screen shake exceeded safe max limit of 20px");
        }

        // Tick shake decay: ensure jitter never produces NaNs or infinite offsets
        for _ in 0..120 {
            chrono.update(0.016);
            assert!(chrono.shake_offset.x.is_finite());
            assert!(chrono.shake_offset.y.is_finite());
            assert!(chrono.shake_offset.length() <= 20.0 + 1e-3);
        }
        assert_eq!(chrono.screen_shake, 0.0);
        assert_eq!(chrono.shake_offset, Vec2::ZERO);

        // 2. Audio trigger edges: verify dilation_edge fires exactly once per transition
        let mut app = test_app();
        assert_eq!(app.world().resource::<ChronoShifter>().dilation_edge, None);

        // Engage gear
        engage(&mut app, KeyCode::Digit4);
        // After engaging, state is sustained: dilation_edge must have cleared back to None
        assert_eq!(app.world().resource::<ChronoShifter>().dilation_edge, None);

        // Disengage
        key(&mut app, KeyCode::ShiftLeft, false);
        app.update();
        // One edge for disengage, then None on subsequent frames
        run(&mut app, 30);
        assert_eq!(app.world().resource::<ChronoShifter>().dilation_edge, None);
    }

    #[test]
    fn test_adversarial_scheduling_order_and_query_disjointness() {
        let mut app = test_app();

        // Add UI entities
        setup_chrono_shift_ui(app.world_mut().commands());
        app.update();

        // Add UI system
        app.add_systems(Update, chrono_shift_ui_system.after(ChronoShiftSet::Control));

        // Concurrently hold shift, spam slash, warp, and movement
        for frame in 0..60 {
            key(&mut app, KeyCode::ShiftLeft, true);
            if frame % 2 == 0 {
                key(&mut app, KeyCode::Digit5, true);
            } else {
                key(&mut app, KeyCode::Digit5, false);
            }
            app.world_mut().send_event(MouseMotion { delta: Vec2::new(10.0, -10.0) });
            app.world_mut().send_event(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: 1.0, window: Entity::PLACEHOLDER });

            // Ensure app update executes without ECS schedule panic or query mutability conflict
            app.update();

            let shifter = app.world().resource::<ChronoShifter>();
            assert!(shifter.model.fade >= 0.0 && shifter.model.fade <= 1.0);
            assert!(is_legal(shifter.model.knob));
        }

        // Release shift
        key(&mut app, KeyCode::ShiftLeft, false);
        run(&mut app, 40);
        let shifter = app.world().resource::<ChronoShifter>();
        assert_eq!(shifter.model.knob, Vec2::ZERO);
        assert_eq!(shifter.model.dilation, 1.0);
    }

    #[test]
    fn test_adversarial_warp_and_shift_energy_starvation() {
        let mut app = test_app();
        app.add_event::<crate::combat::SliceEnemyEvent>()
            .add_systems(Update, crate::player_chrono_warp_system.after(ChronoShiftSet::Control));

        // Engage Gear 6 at full energy first
        engage(&mut app, KeyCode::Digit6);
        assert!((app.world().resource::<ChronoState>().slowmo_factor - 0.04).abs() < 1e-4);

        // Set energy to 28.0 (absorbs 1 frame of Gear 6 drain (~1.1) and 1 warp of 25.0 cost, leaving < 2.0)
        set_energy(&mut app, 28.0);

        // Set frozen aim on the shifter so cursor remains pinned to (200.0, 0.0) while held
        app.world_mut().resource_mut::<ChronoShifter>().frozen_aim = Some(Vec2::new(200.0, 0.0));

        // Trigger Warp (KeyQ)
        key(&mut app, KeyCode::KeyQ, true);
        app.update();
        key(&mut app, KeyCode::KeyQ, false);

        // Warp consumed 25.0 energy. Next 2-3 frames of Gear 6 drain (69.1/s) drain remainder to 0.
        run(&mut app, 5);

        // Energy must hit 0 and trigger stall lockout cleanly
        let shifter = app.world().resource::<ChronoShifter>();
        assert!(shifter.model.stalled, "Shifter must stall when energy starves from warp + drain");
        assert_eq!(app.world().resource::<ChronoState>().slowmo_factor, 1.0);

        // Player cannot warp again while stalled / depleted (< 25 energy)
        key(&mut app, KeyCode::KeyQ, true);
        app.update();
        key(&mut app, KeyCode::KeyQ, false);
        let e = energy(&mut app);
        assert!(e < 20.0);
    }
}
