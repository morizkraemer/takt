# BPM Detector Rust Rewrite with egui + Ableton Link

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Rewrite the Python BPM detector as a standalone Rust app with an egui GUI, Ableton Link tempo sharing, and optional OSC output.

**Architecture:** Audio input via `cpal` feeds a BPM detection engine (bass energy autocorrelation + fine DFT refinement — ported from working Python prototype). Detected BPM is shared via Ableton Link (`rusty_link`) as session leader, and optionally via OSC (`rosc`). An `eframe`/`egui` GUI shows current BPM, audio device selection, BPM range controls, and Link/OSC toggles.

**Tech Stack:** Rust, `cpal` (audio), `rustfft` (FFT), `rusty_link` (Ableton Link), `rosc` (OSC), `eframe`/`egui` (GUI)

**Reference implementation:** `bpm.py` in this repo — the algorithm is proven and should be ported faithfully.

---

## File Structure

```
bpm2/
├── Cargo.toml                  # Dependencies and project config
├── src/
│   ├── main.rs                 # Entry point — launches eframe app
│   ├── app.rs                  # egui App implementation — GUI layout + state
│   ├── audio.rs                # Audio device listing + input stream (cpal)
│   ├── detector.rs             # BPM detection engine (energy, autocorrelation, DFT)
│   ├── link.rs                 # Ableton Link wrapper (rusty_link)
│   └── osc.rs                  # OSC output (rosc)
├── bpm.py                      # Original Python prototype (reference)
├── README.md
├── shed.toml
└── docs/
```

---

### Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [x] **Step 1: Initialize Cargo project**

```bash
cd /Users/morizkraemer/Documents/CODE/tanzverein/bpm2
cargo init --name bpm2
```

- [x] **Step 2: Add dependencies to Cargo.toml**

Replace the generated `Cargo.toml` with:

```toml
[package]
name = "bpm2"
version = "0.1.0"
edition = "2024"

[dependencies]
cpal = "0.15"
rustfft = "6"
eframe = "0.31"
rosc = "0.10"
rusty_link = "0.4"
```

- [x] **Step 3: Write minimal main.rs that opens an egui window**

```rust
mod app;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_title("bpm2"),
        ..Default::default()
    };
    eframe::run_native("bpm2", options, Box::new(|_cc| Ok(Box::new(app::BpmApp::new()))))
}
```

- [x] **Step 4: Create stub app.rs**

```rust
use eframe::egui;

pub struct BpmApp {
    pub current_bpm: f64,
}

impl BpmApp {
    pub fn new() -> Self {
        Self { current_bpm: 0.0 }
    }
}

impl eframe::App for BpmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("bpm2");
            ui.label(format!("BPM: {:.1}", self.current_bpm));
        });
    }
}
```

- [x] **Step 5: Build and run**

```bash
cargo run
```

Expected: A window opens showing "bpm2" heading and "BPM: 0.0".

- [x] **Step 6: Commit**

```bash
git add Cargo.toml src/
git commit -m "feat: scaffold Rust project with egui window"
```

---

### Task 2: Audio Device Listing and Input Stream

**Files:**
- Create: `src/audio.rs`
- Modify: `src/app.rs`
- Modify: `src/main.rs`

- [x] **Step 1: Create audio.rs with device listing**

```rust
use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};

pub struct AudioDevice {
    pub name: String,
    pub index: usize,
}

pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| {
            devices
                .enumerate()
                .filter_map(|(i, d)| {
                    d.name().ok().map(|name| AudioDevice { name, index: i })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct AudioStream {
    _stream: cpal::Stream,
}

impl AudioStream {
    pub fn new(
        device_index: Option<usize>,
        sample_buffer: Arc<Mutex<Vec<f32>>>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let device = match device_index {
            Some(idx) => host
                .input_devices()
                .map_err(|e| e.to_string())?
                .nth(idx)
                .ok_or_else(|| format!("No input device at index {idx}"))?,
            None => host
                .default_input_device()
                .ok_or("No default input device")?,
        };

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(44100),
            buffer_size: cpal::BufferSize::Fixed(128),
        };

        let buffer = sample_buffer.clone();
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                |err| eprintln!("audio error: {err}"),
                None,
            )
            .map_err(|e| e.to_string())?;

        use cpal::traits::StreamTrait;
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self { _stream: stream })
    }
}
```

- [x] **Step 2: Add `mod audio;` to main.rs**

```rust
mod app;
mod audio;
```

- [x] **Step 3: Update app.rs to show device list and allow selection**

```rust
use eframe::egui;
use std::sync::{Arc, Mutex};
use crate::audio::{self, AudioDevice, AudioStream};

pub struct BpmApp {
    pub current_bpm: f64,
    devices: Vec<AudioDevice>,
    selected_device: Option<usize>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<AudioStream>,
    error: Option<String>,
}

impl BpmApp {
    pub fn new() -> Self {
        let devices = audio::list_input_devices();
        Self {
            current_bpm: 0.0,
            devices,
            selected_device: None,
            sample_buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            error: None,
        }
    }

    fn start_stream(&mut self) {
        self.error = None;
        match AudioStream::new(self.selected_device, self.sample_buffer.clone()) {
            Ok(s) => self.stream = Some(s),
            Err(e) => self.error = Some(e),
        }
    }
}

impl eframe::App for BpmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("bpm2");
            ui.add_space(8.0);

            // BPM display
            ui.label(format!("BPM: {:.1}", self.current_bpm));
            ui.add_space(16.0);

            // Device selector
            ui.label("Audio Input:");
            let current_name = self
                .selected_device
                .and_then(|i| self.devices.get(i))
                .map(|d| d.name.as_str())
                .unwrap_or("Default");

            egui::ComboBox::from_id_salt("audio_device")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.selected_device.is_none(), "Default").clicked() {
                        self.selected_device = None;
                        self.start_stream();
                    }
                    for dev in &self.devices {
                        if ui
                            .selectable_label(
                                self.selected_device == Some(dev.index),
                                &dev.name,
                            )
                            .clicked()
                        {
                            self.selected_device = Some(dev.index);
                            self.start_stream();
                        }
                    }
                });

            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::RED, err);
            }

            // Show buffer activity
            if let Ok(buf) = self.sample_buffer.lock() {
                ui.label(format!("Buffer: {} samples", buf.len()));
            }
        });

        // Request repaint for live updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
```

- [x] **Step 4: Build and run**

```bash
cargo run
```

Expected: Window shows device dropdown. Selecting a device starts capturing audio, buffer count increases.

- [x] **Step 5: Commit**

```bash
git add src/audio.rs src/app.rs src/main.rs
git commit -m "feat: audio device listing and input stream via cpal"
```

---

### Task 3: BPM Detection Engine

**Files:**
- Create: `src/detector.rs`
- Modify: `src/main.rs`

This is a faithful port of the proven Python algorithm from `bpm.py`.

- [x] **Step 1: Create detector.rs**

```rust
use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 44100.0;
const FRAME_SIZE: usize = 1024;
const HOP_SIZE: usize = 128;
const HISTORY_SECONDS: f64 = 10.0;
const ONSET_FPS: f64 = SAMPLE_RATE / HOP_SIZE as f64;
const HISTORY_LEN: usize = (HISTORY_SECONDS * ONSET_FPS) as usize;

pub struct BpmDetector {
    bpm_min: f64,
    bpm_max: f64,
    lag_min: usize,
    lag_max: usize,

    // FFT
    fft_planner: FftPlanner<f32>,
    prev_spectrum: Option<Vec<f32>>,

    // Energy history (circular buffer)
    energy_history: Vec<f64>,
    hist_idx: usize,
    hist_filled: bool,

    // Sample accumulator
    sample_buf: Vec<f32>,

    // Output
    current_bpm: f64,

    // Hanning window (precomputed)
    window: Vec<f32>,

    // Bass band bin range
    lo_bin: usize,
    hi_bin: usize,
}

impl BpmDetector {
    pub fn new(bpm_min: f64, bpm_max: f64) -> Self {
        let freq_per_bin = SAMPLE_RATE / FRAME_SIZE as f64;
        let lo_bin = (40.0 / freq_per_bin) as usize;
        let lo_bin = lo_bin.max(1);
        let hi_bin = (200.0 / freq_per_bin) as usize;

        let window: Vec<f32> = (0..FRAME_SIZE)
            .map(|i| {
                let x = 2.0 * PI * i as f64 / (FRAME_SIZE - 1) as f64;
                (0.5 * (1.0 - x.cos())) as f32
            })
            .collect();

        Self {
            bpm_min,
            bpm_max,
            lag_min: (ONSET_FPS * 60.0 / bpm_max) as usize,
            lag_max: (ONSET_FPS * 60.0 / bpm_min) as usize,
            fft_planner: FftPlanner::new(),
            prev_spectrum: None,
            energy_history: vec![0.0; HISTORY_LEN],
            hist_idx: 0,
            hist_filled: false,
            sample_buf: Vec::new(),
            current_bpm: 0.0,
            window,
            lo_bin,
            hi_bin,
        }
    }

    /// Feed raw audio samples into the detector.
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.sample_buf.extend_from_slice(samples);

        while self.sample_buf.len() >= FRAME_SIZE {
            let frame: Vec<f32> = self.sample_buf[..FRAME_SIZE].to_vec();
            self.sample_buf.drain(..HOP_SIZE);

            let energy = self.process_frame(&frame);
            self.energy_history[self.hist_idx] = energy;
            self.hist_idx = (self.hist_idx + 1) % HISTORY_LEN;
            if self.hist_idx == 0 {
                self.hist_filled = true;
            }
        }
    }

    fn process_frame(&mut self, frame: &[f32]) -> f64 {
        // Apply window and prepare FFT input
        let mut buffer: Vec<Complex<f32>> = frame
            .iter()
            .zip(self.window.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();

        // FFT
        let fft = self.fft_planner.plan_fft_forward(FRAME_SIZE);
        fft.process(&mut buffer);

        // Magnitude spectrum (only need first half + 1)
        let spectrum: Vec<f32> = buffer[..FRAME_SIZE / 2 + 1]
            .iter()
            .map(|c| c.norm())
            .collect();

        // Bass band energy
        let hi = self.hi_bin.min(spectrum.len());
        let energy: f64 = spectrum[self.lo_bin..hi]
            .iter()
            .map(|&v| (v as f64) * (v as f64))
            .sum();

        self.prev_spectrum = Some(spectrum);
        energy
    }

    /// Run tempo estimation. Call this periodically (e.g. 4x/sec).
    pub fn estimate_tempo(&mut self) -> f64 {
        let min_samples = (4.0 * ONSET_FPS) as usize;
        if !self.hist_filled && self.hist_idx < min_samples {
            return 0.0;
        }

        // Unroll circular buffer into chronological order
        let signal: Vec<f64> = if self.hist_filled {
            self.energy_history[self.hist_idx..]
                .iter()
                .chain(self.energy_history[..self.hist_idx].iter())
                .copied()
                .collect()
        } else {
            self.energy_history[..self.hist_idx].to_vec()
        };

        let n = signal.len();

        // Normalize
        let mean: f64 = signal.iter().sum::<f64>() / n as f64;
        let signal: Vec<f64> = signal.iter().map(|&v| v - mean).collect();
        let std: f64 = (signal.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
        if std < 1e-10 {
            return 0.0;
        }
        let signal: Vec<f64> = signal.iter().map(|v| v / std).collect();

        // Autocorrelation via FFT (using f64 manually — rustfft is f32,
        // so we do this with direct computation for f64 precision)
        let acf = self.autocorrelate(&signal);

        let lag_min = self.lag_min.max(2);
        let lag_max = self.lag_max.min(n.saturating_sub(2));
        if lag_max <= lag_min {
            return 0.0;
        }

        // Find peak in BPM range
        let region = &acf[lag_min..=lag_max];
        let (peak_idx, &peak_val) = region
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();

        if peak_val < 0.1 {
            return 0.0;
        }

        let coarse_lag = peak_idx + lag_min;
        let coarse_bpm = 60.0 * ONSET_FPS / coarse_lag as f64;

        // Fine DFT search: ±1 BPM in 0.05 steps
        let mut best_bpm = coarse_bpm;
        let mut best_mag = 0.0;

        let mut test_bpm = (coarse_bpm - 1.0).max(self.bpm_min);
        while test_bpm <= (coarse_bpm + 1.0).min(self.bpm_max) {
            let freq = test_bpm / (60.0 * ONSET_FPS);
            // Selective DFT at this frequency
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, &s) in signal.iter().enumerate() {
                let phase = 2.0 * PI * freq * i as f64;
                re += s * phase.cos();
                im += s * phase.sin();
            }
            let mag = (re * re + im * im).sqrt();
            if mag > best_mag {
                best_mag = mag;
                best_bpm = test_bpm;
            }
            test_bpm += 0.05;
        }

        // Smooth output
        let raw = best_bpm;
        if raw > 0.0 {
            if self.current_bpm == 0.0 {
                self.current_bpm = raw;
            } else if (raw - self.current_bpm).abs() > 5.0 {
                self.current_bpm = 0.5 * raw + 0.5 * self.current_bpm;
            } else {
                self.current_bpm = 0.08 * raw + 0.92 * self.current_bpm;
            }
        }
        // Hold last BPM if no detection (never reset to 0)

        self.current_bpm
    }

    fn autocorrelate(&self, signal: &[f64]) -> Vec<f64> {
        let n = signal.len();
        let fft_len = (2 * n).next_power_of_two();

        // Zero-pad and compute via FFT (using f64 precision manually)
        // For large N, use the O(n*log(n)) approach
        let mut padded: Vec<Complex<f64>> = signal
            .iter()
            .map(|&v| Complex::new(v, 0.0))
            .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
            .take(fft_len)
            .collect();

        // In-place FFT (Cooley-Tukey would be ideal but for simplicity,
        // use the direct approach via rustfft with f32 and upcast)
        // Actually, let's use a simple O(n log n) approach
        fft_f64(&mut padded, false);

        // Power spectrum
        for c in padded.iter_mut() {
            *c = Complex::new(c.norm_sqr(), 0.0);
        }

        // Inverse FFT
        fft_f64(&mut padded, true);

        // Normalize by acf[0]
        let acf0 = padded[0].re;
        if acf0 < 1e-10 {
            return vec![0.0; n];
        }

        padded[..n].iter().map(|c| c.re / acf0).collect()
    }

    pub fn current_bpm(&self) -> f64 {
        self.current_bpm
    }

    pub fn buffer_fill(&self) -> (usize, usize) {
        let fill = if self.hist_filled {
            HISTORY_LEN
        } else {
            self.hist_idx
        };
        (fill, HISTORY_LEN)
    }
}

/// Radix-2 Cooley-Tukey FFT for f64 complex.
/// `inverse` = true for inverse FFT.
fn fft_f64(buf: &mut [Complex<f64>], inverse: bool) {
    let n = buf.len();
    assert!(n.is_power_of_two(), "FFT length must be power of 2");

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            buf.swap(i, j);
        }
    }

    // Cooley-Tukey
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle_sign = if inverse { 1.0 } else { -1.0 };
        let angle = angle_sign * 2.0 * PI / len as f64;
        let wn = Complex::new(angle.cos(), angle.sin());

        let mut start = 0;
        while start < n {
            let mut w = Complex::new(1.0, 0.0);
            for k in 0..half {
                let u = buf[start + k];
                let t = w * buf[start + k + half];
                buf[start + k] = u + t;
                buf[start + k + half] = u - t;
                w = w * wn;
            }
            start += len;
        }
        len <<= 1;
    }

    if inverse {
        let inv_n = 1.0 / n as f64;
        for c in buf.iter_mut() {
            *c = *c * inv_n;
        }
    }
}
```

- [x] **Step 2: Add `mod detector;` to main.rs**

```rust
mod app;
mod audio;
mod detector;
```

- [x] **Step 3: Build to verify compilation**

```bash
cargo build
```

Expected: Compiles with no errors.

- [x] **Step 4: Commit**

```bash
git add src/detector.rs src/main.rs
git commit -m "feat: port BPM detection engine from Python to Rust"
```

---

### Task 4: Wire Audio into Detector + Live BPM Display

**Files:**
- Modify: `src/app.rs`

- [x] **Step 1: Integrate detector into BpmApp**

Update `app.rs` to create a detector, drain the sample buffer into it each frame, and display BPM:

```rust
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::audio::{self, AudioDevice, AudioStream};
use crate::detector::BpmDetector;

pub struct BpmApp {
    devices: Vec<AudioDevice>,
    selected_device: Option<usize>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<AudioStream>,
    detector: BpmDetector,
    error: Option<String>,
    bpm_min: f64,
    bpm_max: f64,
    last_estimate: Instant,
}

impl BpmApp {
    pub fn new() -> Self {
        let devices = audio::list_input_devices();
        Self {
            devices,
            selected_device: None,
            sample_buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            detector: BpmDetector::new(120.0, 150.0),
            error: None,
            bpm_min: 120.0,
            bpm_max: 150.0,
            last_estimate: Instant::now(),
        }
    }

    fn start_stream(&mut self) {
        self.error = None;
        match AudioStream::new(self.selected_device, self.sample_buffer.clone()) {
            Ok(s) => self.stream = Some(s),
            Err(e) => self.error = Some(e),
        }
    }

    fn process_audio(&mut self) {
        // Drain sample buffer into detector
        let samples: Vec<f32> = {
            let mut buf = self.sample_buffer.lock().unwrap();
            std::mem::take(&mut *buf)
        };
        if !samples.is_empty() {
            self.detector.push_samples(&samples);
        }

        // Run tempo estimation ~4x/sec
        if self.last_estimate.elapsed().as_millis() >= 250 {
            self.last_estimate = Instant::now();
            self.detector.estimate_tempo();
        }
    }
}

impl eframe::App for BpmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_audio();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("bpm2");
            ui.add_space(8.0);

            // BPM display — large
            let bpm = self.detector.current_bpm();
            if bpm > 0.0 {
                ui.label(
                    egui::RichText::new(format!("{:.1}", bpm))
                        .size(64.0)
                        .strong(),
                );
                ui.label("BPM");
            } else {
                let (fill, total) = self.detector.buffer_fill();
                ui.label(
                    egui::RichText::new("---")
                        .size(64.0)
                        .weak(),
                );
                ui.label(format!("listening... ({fill}/{total})"));
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            // Device selector
            ui.label("Audio Input:");
            let current_name = self
                .selected_device
                .and_then(|i| self.devices.get(i))
                .map(|d| d.name.as_str())
                .unwrap_or("Default");

            egui::ComboBox::from_id_salt("audio_device")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.selected_device.is_none(), "Default").clicked() {
                        self.selected_device = None;
                        self.start_stream();
                    }
                    for dev in &self.devices {
                        if ui
                            .selectable_label(
                                self.selected_device == Some(dev.index),
                                &dev.name,
                            )
                            .clicked()
                        {
                            self.selected_device = Some(dev.index);
                            self.start_stream();
                        }
                    }
                });

            ui.add_space(8.0);

            // BPM range
            ui.horizontal(|ui| {
                ui.label("BPM range:");
                let mut min = self.bpm_min as f32;
                let mut max = self.bpm_max as f32;
                ui.add(egui::DragValue::new(&mut min).range(60.0..=200.0).prefix("min: "));
                ui.add(egui::DragValue::new(&mut max).range(60.0..=200.0).prefix("max: "));
                if min as f64 != self.bpm_min || max as f64 != self.bpm_max {
                    self.bpm_min = min as f64;
                    self.bpm_max = max as f64;
                    self.detector = BpmDetector::new(self.bpm_min, self.bpm_max);
                }
            });

            if let Some(err) = &self.error {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::RED, err);
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}
```

- [x] **Step 2: Build and run**

```bash
cargo run
```

Expected: Window shows live BPM detection. Select an audio device, play music, BPM appears after ~4 seconds.

- [x] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: wire audio input into BPM detector with live display"
```

---

### Task 5: Ableton Link Integration

**Files:**
- Create: `src/link.rs`
- Modify: `src/app.rs`
- Modify: `src/main.rs`

- [x] **Step 1: Create link.rs**

```rust
use rusty_link::{AblLink, SessionState};

pub struct LinkSession {
    link: AblLink,
    enabled: bool,
    quantum: f64,
}

impl LinkSession {
    pub fn new() -> Self {
        let link = AblLink::new(120.0); // initial tempo
        Self {
            link,
            enabled: false,
            quantum: 4.0, // 4 beats per bar
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.link.enable(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_tempo(&self, bpm: f64) {
        if !self.enabled || bpm <= 0.0 {
            return;
        }
        let mut state = SessionState::new();
        self.link.capture_app_session_state(&mut state);
        let time = self.link.clock_micros();
        state.set_tempo(bpm, time);
        self.link.commit_app_session_state(&state);
    }

    pub fn num_peers(&self) -> u64 {
        self.link.num_peers()
    }

    pub fn tempo(&self) -> f64 {
        let mut state = SessionState::new();
        self.link.capture_app_session_state(&mut state);
        state.tempo()
    }
}
```

- [x] **Step 2: Add `mod link;` to main.rs**

```rust
mod app;
mod audio;
mod detector;
mod link;
```

- [x] **Step 3: Integrate Link into app.rs**

Add to `BpmApp` struct fields:

```rust
link: crate::link::LinkSession,
link_enabled: bool,
```

Initialize in `new()`:

```rust
link: crate::link::LinkSession::new(),
link_enabled: false,
```

In `process_audio()`, after `self.detector.estimate_tempo();`, add:

```rust
let bpm = self.detector.current_bpm();
if bpm > 0.0 {
    self.link.set_tempo(bpm);
}
```

Add to the UI (after BPM range section):

```rust
ui.add_space(8.0);
ui.separator();
ui.add_space(8.0);

// Ableton Link
ui.horizontal(|ui| {
    ui.label("Ableton Link:");
    if ui.checkbox(&mut self.link_enabled, "Enable").changed() {
        self.link.set_enabled(self.link_enabled);
    }
    if self.link_enabled {
        let peers = self.link.num_peers();
        ui.label(format!("({} peer{})", peers, if peers == 1 { "" } else { "s" }));
    }
});
```

- [x] **Step 4: Build and run**

```bash
cargo run
```

Expected: Link toggle in GUI. When enabled, other Link-compatible apps on the network see the detected BPM.

- [x] **Step 5: Commit**

```bash
git add src/link.rs src/app.rs src/main.rs
git commit -m "feat: Ableton Link integration as session leader"
```

---

### Task 6: OSC Output

**Files:**
- Create: `src/osc.rs`
- Modify: `src/app.rs`
- Modify: `src/main.rs`

- [x] **Step 1: Create osc.rs**

```rust
use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

pub struct OscSender {
    socket: UdpSocket,
    target: String,
    address: String,
}

impl OscSender {
    pub fn new(ip: &str, port: u16, address: &str) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        Ok(Self {
            socket,
            target: format!("{ip}:{port}"),
            address: address.to_string(),
        })
    }

    pub fn send_bpm(&self, bpm: f64) {
        let msg = OscPacket::Message(OscMessage {
            addr: self.address.clone(),
            args: vec![OscType::Float(bpm as f32)],
        });
        if let Ok(bytes) = encoder::encode(&msg) {
            let _ = self.socket.send_to(&bytes, &self.target);
        }
    }
}
```

- [x] **Step 2: Add `mod osc;` to main.rs**

```rust
mod app;
mod audio;
mod detector;
mod link;
mod osc;
```

- [x] **Step 3: Integrate OSC into app.rs**

Add to `BpmApp` struct fields:

```rust
osc_enabled: bool,
osc_ip: String,
osc_port: String,
osc_address: String,
osc_sender: Option<crate::osc::OscSender>,
```

Initialize in `new()`:

```rust
osc_enabled: false,
osc_ip: "127.0.0.1".to_string(),
osc_port: "9000".to_string(),
osc_address: "/bpm".to_string(),
osc_sender: None,
```

In `process_audio()`, after the Link tempo push, add:

```rust
if let Some(osc) = &self.osc_sender {
    if bpm > 0.0 {
        osc.send_bpm(bpm);
    }
}
```

Add to the UI (after Link section):

```rust
ui.add_space(8.0);
ui.separator();
ui.add_space(8.0);

// OSC
ui.horizontal(|ui| {
    ui.label("OSC Output:");
    if ui.checkbox(&mut self.osc_enabled, "Enable").changed() {
        if self.osc_enabled {
            if let Ok(port) = self.osc_port.parse::<u16>() {
                match crate::osc::OscSender::new(&self.osc_ip, port, &self.osc_address) {
                    Ok(s) => self.osc_sender = Some(s),
                    Err(e) => {
                        self.error = Some(format!("OSC: {e}"));
                        self.osc_enabled = false;
                    }
                }
            }
        } else {
            self.osc_sender = None;
        }
    }
});
if self.osc_enabled || !self.osc_enabled {
    ui.horizontal(|ui| {
        ui.label("IP:");
        ui.add(egui::TextEdit::singleline(&mut self.osc_ip).desired_width(100.0));
        ui.label("Port:");
        ui.add(egui::TextEdit::singleline(&mut self.osc_port).desired_width(50.0));
        ui.label("Address:");
        ui.add(egui::TextEdit::singleline(&mut self.osc_address).desired_width(60.0));
    });
}
```

- [x] **Step 4: Build and run**

```bash
cargo run
```

Expected: OSC toggle and config fields in GUI. When enabled, BPM is sent via UDP/OSC to the configured target.

- [x] **Step 5: Commit**

```bash
git add src/osc.rs src/app.rs src/main.rs
git commit -m "feat: optional OSC output with configurable target"
```

---

### Task 7: Auto-start Audio + Shed Scripts + Polish

**Files:**
- Modify: `src/app.rs`
- Modify: `shed.toml`

- [x] **Step 1: Auto-start audio stream on launch**

In `BpmApp::new()`, after creating the struct, call `start_stream()`:

```rust
let mut app = Self { /* ... */ };
app.start_stream();
app
```

- [x] **Step 2: Update shed.toml**

```toml
[scripts]
bpm = "./venv/bin/python -u bpm.py -d 4"
bpm-test = "./venv/bin/python -u bpm.py -d 4 --no-osc"
bpm-rust = "cargo run --release"
bpm-build = "cargo build --release"
```

- [x] **Step 3: Build release and test**

```bash
cargo build --release
cargo run --release
```

Expected: Fast startup, smooth BPM detection, Link and OSC controls work.

- [x] **Step 4: Commit**

```bash
git add src/app.rs shed.toml
git commit -m "feat: auto-start audio, shed scripts, release build"
```

---

## Summary

| Task | What it builds | Key crate |
|------|---------------|-----------|
| 1 | Project scaffold + egui window | `eframe` |
| 2 | Audio device listing + input | `cpal` |
| 3 | BPM detection engine (ported from Python) | `rustfft` |
| 4 | Wire audio → detector → live display | — |
| 5 | Ableton Link (leader mode) | `rusty_link` |
| 6 | OSC output (optional) | `rosc` |
| 7 | Auto-start, shed scripts, polish | — |
