#!/usr/bin/env python3
"""
Real-time BPM detector for electronic music.
Uses bass energy envelope + autocorrelation with parabolic interpolation.
Outputs BPM via OSC.
"""

import argparse
import sys
import threading
import time

import numpy as np
import sounddevice as sd
from pythonosc import udp_client

# ── Audio config ──────────────────────────────────────────────
SAMPLE_RATE = 44100
FRAME_SIZE = 1024
HOP_SIZE = 128  # ~2.9ms — fine time resolution for <1 BPM accuracy
BLOCK_SIZE = HOP_SIZE

# ── BPM config ────────────────────────────────────────────────
BPM_MIN = 120
BPM_MAX = 150
HISTORY_SECONDS = 10
ONSET_FPS = SAMPLE_RATE / HOP_SIZE  # ~172 frames/sec
HISTORY_LEN = int(HISTORY_SECONDS * ONSET_FPS)

OUTPUT_INTERVAL = 0.25


class BPMDetector:
    def __init__(self, bpm_min=BPM_MIN, bpm_max=BPM_MAX):
        self.bpm_min = bpm_min
        self.bpm_max = bpm_max

        self.energy_history = np.zeros(HISTORY_LEN)
        self.hist_idx = 0
        self.hist_filled = False

        self.current_bpm = 0.0
        self.lock = threading.Lock()
        self.sample_buf = np.zeros(0)

        # Precompute lag range
        self.lag_min = int(ONSET_FPS * 60.0 / bpm_max)
        self.lag_max = int(ONSET_FPS * 60.0 / bpm_min)

    def process_frame(self, frame: np.ndarray) -> float:
        """Compute bass energy for one frame."""
        windowed = frame * np.hanning(len(frame))
        spectrum = np.abs(np.fft.rfft(windowed))

        # Bass band: 40-200 Hz
        freq_per_bin = SAMPLE_RATE / FRAME_SIZE
        lo = max(1, int(40 / freq_per_bin))
        hi = int(200 / freq_per_bin)

        # Energy = sum of squared magnitudes in bass band
        return np.sum(spectrum[lo:hi] ** 2)

    def estimate_tempo(self) -> float:
        """Autocorrelation on bass energy envelope with parabolic interpolation."""
        if not self.hist_filled and self.hist_idx < int(4 * ONSET_FPS):
            return 0.0

        # Unroll circular buffer
        if self.hist_filled:
            signal = np.concatenate([
                self.energy_history[self.hist_idx:],
                self.energy_history[:self.hist_idx],
            ])
        else:
            signal = self.energy_history[:self.hist_idx]

        # Normalize
        signal = signal - np.mean(signal)
        std = np.std(signal)
        if std < 1e-10:
            return 0.0
        signal = signal / std

        # Autocorrelation via FFT
        n = len(signal)
        fft = np.fft.rfft(signal, n=2 * n)
        acf = np.fft.irfft(fft * np.conj(fft))[:n]
        acf = acf / acf[0]

        lag_min = max(self.lag_min, 2)
        lag_max = min(self.lag_max, n - 2)
        if lag_max <= lag_min:
            return 0.0

        region = acf[lag_min:lag_max + 1]
        peak_idx = np.argmax(region)
        peak_val = region[peak_idx]

        if peak_val < 0.1:
            return 0.0

        # Coarse peak in lags
        coarse_lag = peak_idx + lag_min
        # Convert to BPM, then do fine search around it using DFT
        coarse_bpm = 60.0 * ONSET_FPS / coarse_lag

        # Fine search: ±1 BPM in 0.05 steps using selective DFT
        t = np.arange(n)
        fine_bpms = np.arange(coarse_bpm - 1.0, coarse_bpm + 1.05, 0.05)
        fine_bpms = fine_bpms[(fine_bpms >= self.bpm_min) & (fine_bpms <= self.bpm_max)]
        if len(fine_bpms) == 0:
            return coarse_bpm

        freqs = fine_bpms / (60.0 * ONSET_FPS)
        phases = 2.0 * np.pi * np.outer(freqs, t)
        mags = np.abs(np.dot(np.exp(-1j * phases), signal))
        best_idx = np.argmax(mags)

        return float(fine_bpms[best_idx])

    def audio_callback(self, indata, frames, time_info, status):
        if status:
            print(f"audio: {status}", file=sys.stderr, flush=True)

        mono = indata[:, 0] if indata.ndim > 1 else indata.flatten()
        self.sample_buf = np.concatenate([self.sample_buf, mono])

        while len(self.sample_buf) >= FRAME_SIZE:
            frame = self.sample_buf[:FRAME_SIZE]
            self.sample_buf = self.sample_buf[HOP_SIZE:]

            energy = self.process_frame(frame)
            self.energy_history[self.hist_idx] = energy
            self.hist_idx = (self.hist_idx + 1) % HISTORY_LEN
            if self.hist_idx == 0:
                self.hist_filled = True

    def run_output_loop(self, osc_client, osc_address):
        tick = 0
        while True:
            time.sleep(OUTPUT_INTERVAL)
            tick += 1

            bpm = self.estimate_tempo()

            with self.lock:
                if bpm > 0:
                    if self.current_bpm == 0:
                        self.current_bpm = bpm
                    elif abs(bpm - self.current_bpm) > 5.0:
                        # Tempo changed — converge faster
                        self.current_bpm = 0.5 * bpm + 0.5 * self.current_bpm
                    else:
                        # Heavy smoothing to kill jitter
                        self.current_bpm = 0.08 * bpm + 0.92 * self.current_bpm
                    self.miss_count = 0
                else:
                    # Hold last BPM forever — better to show stale BPM than nothing
                    pass

                if self.current_bpm > 0:
                    out_bpm = round(self.current_bpm, 1)
                    print(f"BPM: {out_bpm:6.1f}  (raw: {bpm:.1f})", flush=True)
                    if osc_client:
                        osc_client.send_message(osc_address, float(out_bpm))
                elif tick % 4 == 0:
                    fill = self.hist_idx if not self.hist_filled else HISTORY_LEN
                    print(f"listening... ({fill}/{HISTORY_LEN})", flush=True)


def main():
    parser = argparse.ArgumentParser(description="Real-time BPM detector")
    parser.add_argument("-d", "--device", type=int, default=None, help="Audio input device index")
    parser.add_argument("--list-devices", action="store_true", help="List audio devices and exit")
    parser.add_argument("--osc-ip", default="127.0.0.1", help="OSC target IP (default: 127.0.0.1)")
    parser.add_argument("--osc-port", type=int, default=9000, help="OSC target port (default: 9000)")
    parser.add_argument("--osc-address", default="/bpm", help="OSC address pattern (default: /bpm)")
    parser.add_argument("--no-osc", action="store_true", help="Disable OSC output")
    parser.add_argument("--bpm-min", type=float, default=BPM_MIN, help=f"Min BPM (default: {BPM_MIN})")
    parser.add_argument("--bpm-max", type=float, default=BPM_MAX, help=f"Max BPM (default: {BPM_MAX})")
    args = parser.parse_args()

    if args.list_devices:
        print(sd.query_devices())
        return

    osc_client = None
    if not args.no_osc:
        osc_client = udp_client.SimpleUDPClient(args.osc_ip, args.osc_port)
        print(f"OSC -> {args.osc_ip}:{args.osc_port} {args.osc_address}", flush=True)

    print(f"BPM range: {args.bpm_min}-{args.bpm_max}", flush=True)
    print(f"Device: {args.device or 'default'}", flush=True)
    print(f"Listening... (Ctrl+C to quit)\n", flush=True)

    detector = BPMDetector(bpm_min=args.bpm_min, bpm_max=args.bpm_max)

    output_thread = threading.Thread(
        target=detector.run_output_loop,
        args=(osc_client, args.osc_address),
        daemon=True,
    )
    output_thread.start()

    try:
        with sd.InputStream(
            device=args.device,
            channels=1,
            samplerate=SAMPLE_RATE,
            blocksize=BLOCK_SIZE,
            callback=detector.audio_callback,
        ):
            while True:
                time.sleep(1)
    except KeyboardInterrupt:
        print("\n\nStopped.")
    except sd.PortAudioError as e:
        print(f"\nAudio error: {e}", file=sys.stderr)
        print("Run with --list-devices to see available inputs.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
