# ChronoStick: 2D Stickman Time-Manipulating Combat Action

A fast-paced, dynamic 2D stickman combat arena game built in **Rust** with **Bevy 0.15**, featuring real-time time manipulation, procedural vector graphics, satisfying instagib slashes, ragdoll splitting physics, multi-tier wave escalation, built-in dynamic SFX, and the **Chronos Shift Stick-Shift Time Dilator**.

---

## 🎮 Game Controls

| Key / Input | Action | Description |
| :--- | :--- | :--- |
| **A / D** or **Left / Right** | Run | Crisp acceleration and deceleration (momentum physics) |
| **W** or **Space** | Jump | Variable jump height with coyote time, jump buffer, and upward swoop SFX |
| **E** or **Middle Click** | Chrono Dash | Burst dash with invulnerability, ghost trails, and air burst SFX |
| **Left Mouse Button** or **J** | Katana Slash | High-speed neon blade sweep (1-shot kills enemies) with blade whoosh SFX |
| **Q** or **F** | Chrono Warp | Instant phase-teleport through enemies with sci-fi laser warp SFX |
| **Shift (Hold)** | **Chronos Shift** | **Brings up interactive H-gate stick shift UI to dynamically adjust combat time-dilation** |
| **Mouse Drag** (while Shift held) | Steer Stick | Raw mouse delta steers the gear knob through the machined H-gate |
| **Mouse Wheel** (while Shift held) | Sequential Shift | Shifts up/down through gears sequentially (N ↔ 1 ↔ 2 ↔ 3 ↔ 4 ↔ 5 ↔ 6) |
| **1 – 6** (while Shift held) | Quick Gear Select | Directly shifts the handle to that gear via legal gate pathways |
| **0** or **`** (while Shift held) | Neutral (N) | Returns shifter to Neutral (1.0x normal time) |
| **R** | Restart | Cleanly resets game state, waves, shifter, and chronometrics after defeat |

---

## ⏱️ Chronos Shift: Stick-Shift Time Dilator

Holding **Shift** raises an intricate brass and mechanical gearwork H-gate in the upper-right cockpit HUD. The player operates the stick shift using mouse drag, mouse wheel, or direct gear keys (`1`-`6`).

### 1. H-Pattern Gate Architecture
```text
  1       3       5      ← Top Row (y = +1.0)
  │       │       │
  ├───┬───N───┬───┤      ← Neutral Rail (y = 0.0)
  │       │       │
  2       4       6      ← Bottom Row (y = -1.0)
```

- **Machined Physical Gate**: The knob is strictly constrained to the gate channels; wall collisions and sub-stepped physics prevent tunneling.
- **Detents & Seating**: Releasing mouse drag smoothly snaps the knob into the nearest detent with mechanical micro-shake and haptic visual feedback.
- **In-Gear Persistence on Release**: Once engaged in a gear, releasing the Shift key **leaves the stick in that gear**. The cockpit HUD remains visible at cruising opacity (65%), sustaining time dilation while freeing your mouse and controls for active combat! Returning to Neutral requires holding Shift and pressing `0` or `` ` ``, or stalling at 0 energy.
- **Cursor Anchoring**: While shifting with Shift held, the combat aim vector is frozen at its initial anchor so cursor manipulation within the cockpit does not disturb combat targeting.

### 2. Gear & Time Dilation Ratio Table

| Gear | Position | World Time Scale | Combat Effect | Energy Drain Rate |
| :---: | :---: | :---: | :---: | :---: |
| **N** | `(0, 0)` | **1.00x** | Normal speed | **0 /s** (Passive Regen) |
| **1** | `(-1, +1)` | **0.75x** | Slight reflex assist | **4.7 /s** |
| **2** | `(-1, -1)` | **0.55x** | Tactical cadence | **15.2 /s** |
| **3** | `(0, +1)` | **0.38x** | High-speed dodge window | **28.8 /s** |
| **4** | `(0, -1)` | **0.24x** | Bullet-time choreography | **43.3 /s** |
| **5** | `(+1, +1)` | **0.12x** | Ultra-slow motion cleave | **58.1 /s** |
| **6** | `(+1, -1)` | **0.04x** | **Chronos Stasis (Near Stop)** | **69.1 /s** |

### 3. Continuous Quadratic Meter Drain & Gearbox Stall
- **Nonlinear Drain Function**: Drain scales continuously with dilation:
  $$\text{Drain}(d) = 75 \times (1 - d)^2 \text{ units/sec}$$
  Deeper gears and stronger time dilation consume the Chrono Meter significantly faster.
- **Stall Protection & Hysteresis**:
  - If the Chrono Meter is completely drained to 0% while dilating, the gearbox **stalls** with a red warning flash and grinding camera tremor.
  - The knob is immediately forced back to Neutral (1.0x speed).
  - Shifting is locked out until the player recharges to at least **20% energy** (via kills or passive regen).

### 4. Gear Acclimatization, Flawless Kills & Burnout Mechanics
- **Cadence & Familiarity**: The stickman's nervous system must adapt to higher speeds. The player becomes accustomed to higher gears by executing flawless kills without whiffing attacks or taking damage.
- **Flawless Mastery Requirements**:
  - **Gear 1**: 2 flawless kills to master.
  - **Gear 2 & 3**: 3 flawless kills to master.
  - **Gear 4 & 5**: 4 flawless kills to master.
  - **Gear 6 (Stasis)**: 5 flawless kills to master.
  - Mastered gears illuminate in glowing emerald/gold with `[MASTERED ✨]` in the HUD, locking in the baseline tolerable drain rate ($\times 1.0$).
- **Absurd Burnout Drain on Skipped Tiers**:
  - Shifting into higher gears without mastering previous tiers triggers a massive **burnout multiplier**:
    $$\text{Burnout Multiplier} = 2.5 + (\text{Skipped Tiers} \times 2.0) + (1.0 - \text{Tolerance}) \times 2.0$$
  - Skipping directly to Gear 6 at tier 0 imposes an absurd **$14.5\times$ drain penalty** ($>1,000 \text{ energy/sec}$), vaporizing the entire 100-energy meter in under 100 milliseconds and instantly stalling the gearbox!
  - Flawless kills while skipping ahead only yield partial familiarity ($\le 50\%$) until earlier gears are mastered.
- **Miss Penalties & Composure Loss**:
  - Whiffing a Katana Slash resets your flawless streak and penalizes unmastered gear tolerance by **-30%** (`⚠️ SLASH MISSED - FOCUS LOST!`).
  - Whiffing a Chrono Warp resets your streak and penalizes unmastered gear tolerance by **-40%** (`⚠️ WARP MISSED - FOCUS LOST!`).
  - Taking enemy damage resets your streak and penalizes tolerance by **-20%** (`⚠️ HIT TAKEN - CADENCE BROKEN!`).
  - Mastered gears are permanently locked and cannot be degraded by misses.

---

## 🔊 Audio & SFX Assets (`assets/audio/`)

The game features synthesized retro-futuristic combat SFX via Bevy 0.15's native audio pipeline:
- `slash.wav` — High-speed resonant blade whoosh
- `warp.wav` — Cyberpunk phase-teleport laser frequency chirp + sub-bass
- `slowmo_enter.wav` — Cinematic time dilation sub-bass pitch bend
- `slowmo_exit.wav` — Time resume rising sweep
- `slice_kill.wav` — Heavy visceral impact crunch + cleave
- `dash.wav` — Compressed air burst
- `jump.wav` — Upward harmonic jump spring

---

## 🚀 Running the Game

```powershell
.\run.bat
```
Or with Cargo:
```powershell
cargo run
```

---

## 🧪 Automated Test Suite

Run the full suite of unit, kinematic, fuzzing, and adversarial stress tests:

```bash
cargo test
```

All 50 tests pass cleanly:
- 24 Core Gameplay tests (kinematics, jump curves, coyote time, katana hitboxes, ragdoll physics, wave escalation, anti-deadlock, ragdoll entity lifecycle).
- 26 Chronos Shift & Acclimatization tests (fuzzing gate boundaries, wall collisions, anti-tunneling, gear persistence on release, sequential wheel shifts, planned path finding, quadratic drain math, gear acclimatization progression via flawless kills, miss penalties and streak resets, consecutive vs skipped tier burnout multipliers, stall hysteresis, and full Bevy App ECS integration).
