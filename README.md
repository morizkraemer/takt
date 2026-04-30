# takt — Real-time BPM Detector

Real-time BPM detection from a live audio stream. Available as a standalone Rust app with egui GUI, Ableton Link tempo sharing, and optional OSC output.

## Features

- **Live BPM detection** — Bass energy autocorrelation + fine DFT refinement (ported from proven Python prototype)
- **egui GUI** — Audio device selection, BPM range controls, real-time BPM display
- **Ableton Link** — Share detected tempo as session leader with other Link-compatible apps
- **OSC output** — Optional OSC to Chataigne, QLC+, or any OSC-capable software

## How it works

1. **Audio capture** — Reads audio from a system input device via `cpal` (PortAudio)
2. **Bass energy extraction** — Each 1024-sample frame is windowed + FFT'd, and the energy in the 40-200 Hz band is computed
3. **Autocorrelation** — A rolling 10-second buffer of bass energy values is autocorrelated to find the dominant periodicity
4. **Fine DFT search** — The coarse autocorrelation peak is refined with a selective DFT at 0.05 BPM resolution (±1 BPM around the peak)
5. **Smoothing** — Output is exponentially smoothed (0.08/0.92 ratio) to prevent jitter. Larger tempo jumps (>5 BPM) converge faster
6. **Hold behavior** — Once locked on, the BPM is held indefinitely through breakdowns/quiet sections

## Tech Stack

| Component | Crate |
|-----------|-------|
| Audio I/O | `cpal` |
| FFT | `rustfft` + custom Cooley-Tukey (f64) |
| GUI | `eframe` / `egui` |
| Ableton Link | `rusty_link` |
| OSC | `rosc` |

## Setup

```bash
cd takt
cargo build --release
```

## Usage

```bash
# GUI app (auto-starts audio)
cargo run --release

# Or via shed
shed run bpm-rust
```

### shed scripts

```bash
shed run bpm        # Original Python with OSC (BlackHole device 4)
shed run bpm-test   # Original Python, no OSC
shed run bpm-rust   # Rust GUI app (--release)
shed run bpm-build  # Build Rust release binary
```

## OSC Output

Toggle OSC in the GUI and configure:

- **Address:** `/bpm` (default, configurable)
- **Value:** `float` — current BPM (e.g. `128.3`)
- **Target:** `127.0.0.1:9000` (default, configurable)

## Architecture

```
takt/
├── Cargo.toml      # Dependencies and project config
├── src/
│   ├── main.rs     # Entry point — launches eframe app
│   ├── app.rs      # egui App — GUI layout + state management
│   ├── audio.rs    # Audio device listing + input stream (cpal)
│   ├── detector.rs # BPM detection engine (energy, autocorrelation, DFT)
│   ├── link.rs     # Ableton Link wrapper (rusty_link)
│   └── osc.rs      # OSC output (rosc)
├── bpm.py          # Original Python prototype (reference)
├── README.md
── shed.toml
└── docs/
```

## Tuning

Key constants in `detector.rs`:

- `BPM_MIN` / `BPM_MAX` — Constrains detection range (also configurable in GUI)
- `HISTORY_SECONDS` (30) — How many seconds of audio to analyze
- `HOP_SIZE` (128) — Time resolution
- Smoothing (0.08/0.92) in `estimate_tempo()` — Lower first value = more stable

## Original Python Prototype

The Python reference implementation is still available as `bpm.py`.
