# ChronoStick: 2D Stickman Time-Manipulating Combat Action

A fast-paced, dynamic 2D stickman combat arena game built in **Rust** with **Bevy 0.15**, featuring real-time time manipulation, procedural vector graphics, satisfying instagib slashes, ragdoll splitting physics, and multi-tier wave escalation.

---

## 🎮 Game Controls

| Key / Input | Action | Description |
| :--- | :--- | :--- |
| **A / D** or **Left / Right** | Run | Crisp acceleration and deceleration (momentum physics) |
| **W** or **Space** | Jump | Variable jump height with coyote time and jump buffering |
| **Left Shift** | Dash | Instant burst dash with invulnerability and ghost trails |
| **Left Mouse Button** or **J** | Katana Slash | High-speed neon blade sweep (1-shot kills enemies) |
| **Right Mouse Button** or **Shift** | Chrono Dilation | Slows down world time (enemies, attacks) to **0.15x** while you move fast |
| **Q** or **F** or **Middle Click** | Chrono Warp | Instant phase-teleport through enemies along your cursor aim line; slices everything in your path |
| **R** | Restart | Cleanly resets game state, waves, and chronometrics after defeat |

---

## ⚡ Mechanics & Features

### 1. Time Manipulation Powers
- **Chrono Dilation (Bullet Time)**:
  - Drains Chrono Energy while held.
  - World time drops to $15\%$ speed, causing enemy attacks and charges to slow to a crawl.
  - The player's personal time scale remains fast, allowing precise dodging and positioning.
- **Chrono Warp (Teleport Slash)**:
  - Teleports you instantaneously toward your mouse cursor (up to 380px range).
  - Any enemy stickman intersected by the warp trajectory is cleaved in half in 1 shot with a cyan laser slice effect.
  - Generates multi-kill bonus multipliers for slicing multiple enemies in a single teleport!
- **Chrono Energy Meter**:
  - Max capacity: 100 energy units.
  - Slaying an enemy rewards **+25 Chrono Energy** immediately.
  - Passive regeneration when not warping or dilating time.

### 2. Combat & Instagib Feel
- **1-Shot Katana Cleave**:
  - Clean vector sword sweep.
  - Every slain stickman triggers a 40ms **Hitstop** (screen freeze impact feel) and screen shake.
  - Severed stickmen split into two physics-driven ragdoll pieces (upper torso and lower limbs) that tumble apart with realistic momentum.
  - High-velocity blood and spark particles bounce against arena platforms with restitution and friction.
- **Combo System**:
  - Continuous kills build a combo multiplier up to **5x**, escalating your wave score.

### 3. Dynamic Stickman Kinematics
- Smooth horizontal momentum with dedicated acceleration and deceleration curves.
- Custom 2D platformer jump curve:
  - **Coyote Time** (120ms): Jump cleanly even after walking off a platform edge.
  - **Jump Buffer** (120ms): Queue jumps slightly before landing.
  - **Variable Gravity**: Releasing the jump button early applies extra short-hop gravity ($2.2\times$). Falling velocity scales up ($1.6\times$) for punchy landings without floatiness.

### 4. Wave Escalation System
- **Wave Director**:
  - Enemies spawn from dynamic left and right arena portals.
  - **Enemy Archetypes**:
    - **Runner**: Fast, agile stickman that swarms and lunges.
    - **Ninja**: High-leaping acrobat stickman with rapid strikes.
    - **Heavy**: Armored stickman wielding a greatsword with broad windups.
  - Enemy AI states: Spawning, Chase, Windup (red laser telegraph line), and Attack.
  - Automatic abyss recovery ensures enemies knocked outside boundaries return safely without wave stalls.

---

## 🚀 Running the Game

Ensure you have Rust installed. From the repository root, run:

```bash
cargo run
```

Or run the compiled binary directly:

```bash
.\target\debug\chrono_stick.exe
```

---

## 🧪 Automated Test Suite

Run the 24 automated unit and adversarial stress tests:

```bash
cargo test
```

All 24 tests cover:
- Kinematic acceleration and friction decay
- Variable jump gravity and coyote/buffer timings
- Time dilation math and hitstop clamping
- Teleport ray-segment and katana arc collision geometry
- Slice ragdoll physics split momentum and non-NaN checks
- Adversarial tests: intra-frame event deduplication, ragdoll entity despawn leak validation, wave manager zero-remainder deadlock protection, and abyss out-of-bounds recovery.
