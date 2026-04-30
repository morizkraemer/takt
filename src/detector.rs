use rustfft::num_complex::Complex;
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 44100.0;
const FRAME_SIZE: usize = 1024;
const HOP_SIZE: usize = 128;
const HISTORY_SECONDS: f64 = 30.0;
const ONSET_FPS: f64 = SAMPLE_RATE / HOP_SIZE as f64;
const HISTORY_LEN: usize = (HISTORY_SECONDS * ONSET_FPS) as usize;

pub struct BpmDetector {
    bpm_min: f64,
    bpm_max: f64,
    lag_min: usize,
    lag_max: usize,

    // FFT
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
            .map(|(s, w)| Complex::new(*s * *w, 0.0))
            .collect();

        // FFT
        let mut planner = rustfft::FftPlanner::new();
        let fft = planner.plan_fft_forward(FRAME_SIZE);
        fft.process(&mut buffer);

        // Magnitude spectrum (only need first half + 1)
        let spectrum: Vec<f32> = buffer[..FRAME_SIZE / 2 + 1].iter().map(|c| c.norm()).collect();

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
        let signal: Vec<f64> = signal.iter().map(|&v| v / std).collect();

        // Autocorrelation
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
                // Tempo jump — converge faster
                self.current_bpm = 0.5 * raw + 0.5 * self.current_bpm;
            } else {
                // Heavy smoothing to kill jitter
                self.current_bpm = 0.08 * raw + 0.92 * self.current_bpm;
            }
        }
        // Hold last BPM if no detection (never reset to 0)

        self.current_bpm
    }

    fn autocorrelate(&self, signal: &[f64]) -> Vec<f64> {
        let n = signal.len();
        let fft_len = (2 * n).next_power_of_two();

        // Zero-pad
        let mut padded: Vec<Complex<f64>> = signal
            .iter()
            .map(|&v| Complex::new(v, 0.0))
            .chain(std::iter::repeat_with(|| Complex::new(0.0, 0.0)))
            .take(fft_len)
            .collect();

        // Forward FFT
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

    // Cooley-Tukey butterfly
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
