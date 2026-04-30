use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::audio::{self, AudioDevice, AudioStream};
use crate::detector::BpmDetector;

#[derive(PartialEq)]
enum OscMode {
    Raw,
    Chromatik,
}

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

    // Ableton Link
    link: crate::link::LinkSession,
    link_enabled: bool,

    // OSC
    osc_enabled: bool,
    osc_mode: OscMode,
    osc_ip: String,
    osc_port: String,
    osc_address: String,
    osc_sender: Option<crate::osc::OscSender>,

    // Beat phase tracking (Chromatik mode)
    beat_phase: f64,
    beat_in_bar: i32,
    last_bpm: f64,
    last_process: Instant,
}

impl BpmApp {
    pub fn new() -> Self {
        let devices = audio::list_input_devices();
        let mut app = Self {
            devices,
            selected_device: None,
            sample_buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            detector: BpmDetector::new(120.0, 150.0),
            error: None,
            bpm_min: 120.0,
            bpm_max: 150.0,
            last_estimate: Instant::now(),
            link: crate::link::LinkSession::new(),
            link_enabled: false,
            osc_enabled: false,
            osc_mode: OscMode::Chromatik,
            osc_ip: "127.0.0.1".to_string(),
            osc_port: "9000".to_string(),
            osc_address: "/bpm".to_string(),
            osc_sender: None,
            beat_phase: 0.0,
            beat_in_bar: 1,
            last_bpm: 0.0,
            last_process: Instant::now(),
        };
        app.start_stream();
        app
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

        // Track elapsed time for beat phase
        let now = Instant::now();
        let dt = now.duration_since(self.last_process).as_secs_f64();
        self.last_process = now;

        // Push BPM to Link and OSC
        let bpm = self.detector.current_bpm();
        if bpm > 0.0 {
            self.link.set_tempo(bpm);

            if let Some(osc) = &self.osc_sender {
                match self.osc_mode {
                    OscMode::Raw => {
                        osc.send_bpm_raw(&self.osc_address, bpm);
                    }
                    OscMode::Chromatik => {
                        // Send BPM only when it changes noticeably
                        if (bpm - self.last_bpm).abs() > 0.05 {
                            osc.send_bpm(bpm);
                            self.last_bpm = bpm;
                        }

                        // Beat phase tracking using real elapsed time
                        let beat_interval = 60.0 / bpm;
                        self.beat_phase += dt;
                        while self.beat_phase >= beat_interval {
                            self.beat_phase -= beat_interval;
                            self.beat_in_bar = (self.beat_in_bar % 4) + 1;
                            osc.send_beat();
                            osc.send_beat_within_bar(self.beat_in_bar);
                        }
                    }
                }
            }
        } else {
            // Reset beat tracking when no BPM
            self.beat_phase = 0.0;
            self.beat_in_bar = 1;
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

                // Beat indicator in Chromatik mode
                if self.osc_mode == OscMode::Chromatik && self.osc_enabled {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for b in 1..=4 {
                            let active = b == self.beat_in_bar;
                            let style = if active {
                                egui::RichText::new(format!("{b}")).strong().size(20.0)
                            } else {
                                egui::RichText::new(format!("{b}")).weak().size(20.0)
                            };
                            ui.label(style);
                        }
                    });
                }
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

            let mut selection_changed = false;

            egui::ComboBox::from_id_salt("audio_device")
                .selected_text(current_name)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.selected_device.is_none(), "Default").clicked() {
                        self.selected_device = None;
                        selection_changed = true;
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
                            selection_changed = true;
                        }
                    }
                });

            if selection_changed {
                self.start_stream();
            }

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
                    ui.label(format!(
                        "({} peer{})",
                        peers,
                        if peers == 1 { "" } else { "s" }
                    ));
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // OSC
            ui.horizontal(|ui| {
                ui.label("OSC Output:");
                if ui.checkbox(&mut self.osc_enabled, "Enable").changed() {
                    if self.osc_enabled {
                        self.recreate_osc_sender();
                    } else {
                        self.osc_sender = None;
                    }
                }
            });

            // OSC mode selector
            ui.horizontal(|ui| {
                ui.label("Mode:");
                ui.radio_value(&mut self.osc_mode, OscMode::Chromatik, "Chromatik");
                ui.radio_value(&mut self.osc_mode, OscMode::Raw, "Raw");
            });

            // OSC connection fields
            ui.horizontal(|ui| {
                ui.label("IP:");
                ui.add(egui::TextEdit::singleline(&mut self.osc_ip).desired_width(100.0));
                ui.label("Port:");
                ui.add(egui::TextEdit::singleline(&mut self.osc_port).desired_width(50.0));
            });

            if self.osc_mode == OscMode::Raw {
                ui.horizontal(|ui| {
                    ui.label("Address:");
                    ui.add(egui::TextEdit::singleline(&mut self.osc_address).desired_width(100.0));
                });
            } else {
                ui.horizontal(|ui| {
                    ui.weak("Sends: /lx/tempo/bpm, /lx/tempo/beat, /lx/tempo/beat-within-bar");
                });
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

impl BpmApp {
    fn recreate_osc_sender(&mut self) {
        if let Ok(port) = self.osc_port.parse::<u16>() {
            match crate::osc::OscSender::new(&self.osc_ip, port) {
                Ok(s) => self.osc_sender = Some(s),
                Err(e) => {
                    self.error = Some(format!("OSC: {e}"));
                    self.osc_enabled = false;
                }
            }
        }
    }
}
