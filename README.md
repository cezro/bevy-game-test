# ChronoStick: 2D Stickman Time-Manipulating Combat Action

A fast-paced, dynamic 2D stickman combat arena game built in **Rust** with **Bevy 0.15**, featuring real-time time manipulation, procedural vector graphics, satisfying instagib slashes, ragdoll splitting physics, multi-tier wave escalation, and built-in dynamic SFX.

---

## 🎮 Game Controls

| Key / Input | Action | Description |
| :--- | :--- | :--- |
| **A / D** or **Left / Right** | Run | Crisp acceleration and deceleration (momentum physics) |
| **W** or **Space** | Jump | Variable jump height with coyote time, jump buffer, and upward swoop SFX |
| **Left Shift** | Dash | Burst dash with invulnerability, ghost trails, and air burst SFX |
| **Left Mouse Button** or **J** | Katana Slash | High-speed neon blade sweep (1-shot kills enemies) with blade whoosh SFX |
| **Right Mouse Button** or **Shift** | Chrono Dilation | Slows down world time to **0.15x** with cinematic sub-bass pitch drop SFX |
| **Q** or **F** or **Middle Click** | Chrono Warp | Instant phase-teleport through enemies with sci-fi laser warp SFX |
| **R** | Restart | Cleanly resets game state, waves, and chronometrics after defeat |

---

## 🔊 Audio & Public SFX Libraries

The game now has the Bevy 0.15 native audio pipeline (`bevy_audio`, `wav`, `vorbis`) configured and integrated.

### Built-in SFX Assets (`assets/audio/`)
- `slash.wav` — High-speed resonant blade whoosh
- `warp.wav` — Cyberpunk phase-teleport laser frequency chirp + sub-bass
- `slowmo_enter.wav` — Cinematic time dilation sub-bass pitch bend
- `slowmo_exit.wav` — Time resume rising sweep
- `slice_kill.wav` — Heavy visceral impact crunch + cleave
- `dash.wav` — Compressed air burst
- `jump.wav` — Upward harmonic jump spring

### Recommended Public / Free Audio Libraries:
1. **Kenney Game Assets (CC0 - Public Domain)**
   - [Kenney Sci-Fi Sounds](https://kenney.nl/assets/sci-fi-sounds): 100+ laser, phase, time warp, hums.
   - [Kenney Impact Sounds](https://kenney.nl/assets/impact-sounds): Slashes, chops, hits, metal.
   - [Kenney Digital Audio](https://kenney.nl/assets/digital-audio): Retro synth, futuristic bleeps.
2. **OpenGameArt.org (CC0)**
   - Sword swipes, slashes, laser dashes, and gore/slice impacts.
3. **Freesound.org (CC0 / Public Domain)**
   - Huge community library of bullet-time whooshes, mechanical impacts, and cyber zaps.
4. **Sonniss GDC Archive (Royalty-Free / Commercial)**
   - Massive collection of professional game audio packs released annually under CC-equivalent royalty-free licenses.

To replace or customize any sound, drop any 44.1kHz `.wav` or `.ogg` file into the `assets/audio/` folder!

---

## 🚀 Running the Game

```powershell
.\run.bat
```
Or:
```powershell
cargo run
```
Or execute the binary directly:
```powershell
.\target\debug\chrono_stick.exe
```

---

## 🧪 Automated Test Suite

```bash
cargo test
```
All 24 automated unit and adversarial stress tests pass with 0 warnings.
