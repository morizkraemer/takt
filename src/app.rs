use eframe::egui;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use crate::audio::{self, AudioDevice, AudioStream};
use crate::detector::BpmDetector;

#[derive(PartialEq, Clone, Serialize, Deserialize)]
enum OscMode {
    Chromatik,
    Raw,
}

#[derive(PartialEq, Clone, Serialize, Deserialize)]
enum SendFormat {
    Number,
    String,
}

#[derive(Clone)]
struct OscTarget {
    mode: OscMode,
    ip: String,
    port: String,
    address: String, // only used in Raw mode
    send_format: SendFormat,
    enabled: bool,
    sender: Option<crate::osc::OscSender>,
}

impl OscTarget {
    fn chromatik() -> Self {
        Self {
            mode: OscMode::Chromatik,
            ip: "127.0.0.1".to_string(),
            port: "9000".to_string(),
            address: "/bpm".to_string(),
            send_format: SendFormat::Number,
            enabled: true,
            sender: None,
        }
    }

    fn raw() -> Self {
        Self {
            mode: OscMode::Raw,
            ip: "127.0.0.1".to_string(),
            port: "8000".to_string(),
            address: "/bpm".to_string(),
            send_format: SendFormat::Number,
            enabled: true,
            sender: None,
        }
    }

    fn to_settings(&self) -> TargetSettings {
        TargetSettings {
            mode: self.mode.clone(),
            ip: self.ip.clone(),
            port: self.port.clone(),
            address: self.address.clone(),
            send_format: self.send_format.clone(),
            enabled: self.enabled,
            connected: self.sender.is_some(),
        }
    }

    fn from_settings(t: &TargetSettings) -> Self {
        let mut target = Self {
            mode: t.mode.clone(),
            ip: t.ip.clone(),
            port: t.port.clone(),
            address: t.address.clone(),
            send_format: t.send_format.clone(),
            enabled: t.enabled,
            sender: None,
        };
        if t.connected {
            // Failure is silent here; the indicator simply stays off.
            let _ = target.connect();
        }
        target
    }

    fn connect(&mut self) -> Result<(), String> {
        if !self.enabled {
            self.sender = None;
            return Ok(());
        }
        let port: u16 = self.port.parse().map_err(|_| format!("Invalid port: {}", self.port))?;
        match crate::osc::OscSender::new(&self.ip, port) {
            Ok(s) => {
                self.sender = Some(s);
                Ok(())
            }
            Err(e) => {
                self.sender = None;
                Err(e)
            }
        }
    }
}

/// Persisted form of an OSC target (runtime sender is rebuilt on restore).
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct TargetSettings {
    mode: OscMode,
    ip: String,
    port: String,
    address: String,
    send_format: SendFormat,
    enabled: bool,
    /// Was connected when the settings were saved.
    connected: bool,
}

impl Default for TargetSettings {
    fn default() -> Self {
        OscTarget::chromatik().to_settings()
    }
}

/// Everything the user can set; restored on next launch.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    audio_device: Option<String>,
    bpm_min: f64,
    bpm_max: f64,
    targets: Vec<TargetSettings>,
    osc_in_enabled: bool,
    osc_in_port: String,
    link_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_device: None,
            bpm_min: 120.0,
            bpm_max: 150.0,
            targets: vec![OscTarget::chromatik().to_settings()],
            osc_in_enabled: false,
            osc_in_port: "9001".to_string(),
            link_enabled: false,
        }
    }
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

    // OSC targets (multiple)
    osc_targets: Vec<OscTarget>,

    // Beat phase tracking (Chromatik mode) — per target
    beat_phases: Vec<f64>,
    beat_in_bars: Vec<i32>,
    last_bpms: Vec<f64>,
    last_process: Instant,

    // OSC Input
    osc_in_enabled: bool,
    osc_in_port: String,
    osc_in_receiver: Option<std::sync::mpsc::Receiver<crate::osc::OscCommand>>,
    osc_in_listener: Option<crate::osc::OscReceiver>,

    // Feedback
    osc_reset_flash: Option<Instant>,
}

impl BpmApp {
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Self {
        let settings: Settings = storage
            .and_then(|st| eframe::get_value(st, eframe::APP_KEY))
            .unwrap_or_default();

        let devices = audio::list_input_devices();
        // Restore device by name; silently fall back to default if missing.
        let selected_device = settings
            .audio_device
            .as_ref()
            .and_then(|name| devices.iter().find(|d| &d.name == name))
            .map(|d| d.index);

        let (bpm_min, bpm_max) = if settings.bpm_min >= 60.0
            && settings.bpm_max <= 200.0
            && settings.bpm_min < settings.bpm_max
        {
            (settings.bpm_min, settings.bpm_max)
        } else {
            (120.0, 150.0)
        };

        let osc_targets: Vec<OscTarget> = if settings.targets.is_empty() {
            vec![OscTarget::chromatik()]
        } else {
            settings.targets.iter().map(OscTarget::from_settings).collect()
        };
        let n_targets = osc_targets.len();

        let mut app = Self {
            devices,
            selected_device,
            sample_buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            detector: BpmDetector::new(bpm_min, bpm_max),
            error: None,
            bpm_min,
            bpm_max,
            last_estimate: Instant::now(),
            link: crate::link::LinkSession::new(),
            link_enabled: false,
            osc_targets,
            beat_phases: vec![0.0; n_targets],
            beat_in_bars: vec![1; n_targets],
            last_bpms: vec![0.0; n_targets],
            last_process: Instant::now(),
            osc_in_enabled: false,
            osc_in_port: settings.osc_in_port.clone(),
            osc_in_receiver: None,
            osc_in_listener: None,
            osc_reset_flash: None,
        };
        app.start_stream();

        if settings.link_enabled {
            app.link_enabled = true;
            app.link.set_enabled(true);
        }
        // Single bind at startup; listener is only started here, once.
        if settings.osc_in_enabled {
            app.osc_in_enabled = true;
            app.start_osc_listener();
        }
        app
    }

    fn to_settings(&self) -> Settings {
        Settings {
            audio_device: self
                .selected_device
                .and_then(|i| self.devices.iter().find(|d| d.index == i))
                .map(|d| d.name.clone()),
            bpm_min: self.bpm_min,
            bpm_max: self.bpm_max,
            targets: self.osc_targets.iter().map(OscTarget::to_settings).collect(),
            osc_in_enabled: self.osc_in_enabled,
            osc_in_port: self.osc_in_port.clone(),
            link_enabled: self.link_enabled,
        }
    }

    fn start_stream(&mut self) {
        self.error = None;
        match AudioStream::new(self.selected_device, self.sample_buffer.clone()) {
            Ok(s) => self.stream = Some(s),
            Err(e) => self.error = Some(e),
        }
    }

    fn start_osc_listener(&mut self) {
        match self.osc_in_port.parse::<u16>() {
            Ok(port) => {
                match crate::osc::OscReceiver::new(port) {
                    Ok((listener, rx)) => {
                        self.osc_in_listener = Some(listener);
                        self.osc_in_receiver = Some(rx);
                    }
                    Err(e) => {
                        self.error = Some(format!("OSC listen: {e}"));
                        self.osc_in_enabled = false;
                    }
                }
            }
            Err(_) => {
                self.error = Some(format!("Invalid OSC port: {}", self.osc_in_port));
                self.osc_in_enabled = false;
            }
        }
    }

    fn process_osc_input(&mut self) {
        let commands: Vec<crate::osc::OscCommand> = if let Some(rx) = &self.osc_in_receiver {
            rx.try_iter().collect()
        } else {
            Vec::new()
        };
        for cmd in commands {
            match cmd {
                crate::osc::OscCommand::Reset => {
                    self.reset_detector();
                    self.osc_reset_flash = Some(Instant::now());
                }
            }
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

            for (i, target) in self.osc_targets.iter().enumerate() {
                if let Some(osc) = &target.sender {
                    match target.mode {
                        OscMode::Raw => {
                            match target.send_format {
                                SendFormat::Number => {
                                    osc.send_bpm_raw(&target.address, bpm);
                                }
                                SendFormat::String => {
                                    osc.send_string(&target.address, &format!("{:.1}", bpm));
                                }
                            }
                        }
                        OscMode::Chromatik => {
                            // BPM as number
                            if (bpm - self.last_bpms[i]).abs() > 0.05 {
                                match target.send_format {
                                    SendFormat::Number => {
                                        osc.send_bpm(bpm);
                                    }
                                    SendFormat::String => {
                                        osc.send_string("/lx/tempo/bpm", &format!("{:.1}", bpm));
                                    }
                                }
                                self.last_bpms[i] = bpm;
                            }

                            // Beat phase tracking
                            let beat_interval = 60.0 / bpm;
                            self.beat_phases[i] += dt;
                            while self.beat_phases[i] >= beat_interval {
                                self.beat_phases[i] -= beat_interval;
                                self.beat_in_bars[i] = (self.beat_in_bars[i] % 4) + 1;
                                osc.send_beat();
                                osc.send_beat_within_bar(self.beat_in_bars[i]);
                            }
                        }
                    }
                }
            }
        } else {
            // No BPM — send "recalculating" for string modes
            for target in self.osc_targets.iter() {
                if let Some(osc) = &target.sender {
                    if target.send_format == SendFormat::String {
                        let addr = match target.mode {
                            OscMode::Raw => target.address.clone(),
                            OscMode::Chromatik => "/lx/tempo/bpm".to_string(),
                        };
                        osc.send_string(&addr, "recalculating");
                    }
                }
            }
            // Reset beat tracking when no BPM
            for i in 0..self.osc_targets.len() {
                self.beat_phases[i] = 0.0;
                self.beat_in_bars[i] = 1;
            }
        }
    }

    fn reset_detector(&mut self) {
        self.detector.reset();
        for i in 0..self.osc_targets.len() {
            self.beat_phases[i] = 0.0;
            self.beat_in_bars[i] = 1;
            self.last_bpms[i] = 0.0;
        }
    }
}

impl eframe::App for BpmApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.to_settings());
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(10)
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_osc_input();
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

            // Reset button
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Reset").clicked() {
                    self.reset_detector();
                }
                ui.label(egui::RichText::new("Clear history & recalculate").weak().small());
                if let Some(t) = self.osc_reset_flash {
                    if t.elapsed().as_secs() < 2 {
                        ui.colored_label(egui::Color32::GREEN, "OSC reset!");
                    } else {
                        self.osc_reset_flash = None;
                    }
                }
            });

            // Beat indicator — show for any Chromatik target that's connected
            let has_chromatik = self.osc_targets.iter().any(|t| t.mode == OscMode::Chromatik && t.sender.is_some());
            if has_chromatik && bpm > 0.0 {
                // Show beat from first active chromatik target
                let first_chromatik = self.osc_targets.iter().position(|t| t.mode == OscMode::Chromatik && t.sender.is_some());
                if let Some(idx) = first_chromatik {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for b in 1..=4 {
                            let active = b == self.beat_in_bars[idx];
                            let style = if active {
                                egui::RichText::new(format!("{b}")).strong().size(20.0)
                            } else {
                                egui::RichText::new(format!("{b}")).weak().size(20.0)
                            };
                            ui.label(style);
                        }
                    });
                }
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
                    let current_bpm = self.detector.current_bpm();
                    self.detector = BpmDetector::new(self.bpm_min, self.bpm_max);
                    // Preserve the last BPM estimate
                    if current_bpm > 0.0 {
                        self.detector.reset();
                        // We can't set current_bpm directly, so it'll re-detect
                    }
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

            // OSC Targets
            ui.label("OSC Output:");

            // Collect indices to remove (can't mutate while iterating)
            let mut to_remove = None;
            let mut connect_idx = None;

            let target_count = self.osc_targets.len();
            for (i, target) in self.osc_targets.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut target.enabled, "");
                        ui.radio_value(&mut target.mode, OscMode::Chromatik, "Chromatik");
                        ui.radio_value(&mut target.mode, OscMode::Raw, "Raw");

                        if target_count > 1 {
                            ui.add_space(8.0);
                            if ui.small_button("✕").clicked() {
                                to_remove = Some(i);
                            }
                        }
                    });

                    // Send format toggle
                    ui.horizontal(|ui| {
                        ui.label("Send:");
                        ui.radio_value(&mut target.send_format, SendFormat::Number, "Number");
                        ui.radio_value(&mut target.send_format, SendFormat::String, "String");
                    });

                    ui.horizontal(|ui| {
                        ui.label("IP:");
                        ui.add(egui::TextEdit::singleline(&mut target.ip).desired_width(100.0));
                        ui.label("Port:");
                        ui.add(egui::TextEdit::singleline(&mut target.port).desired_width(50.0));
                        if ui.small_button("Connect").clicked() {
                            connect_idx = Some(i);
                        }
                        if target.sender.is_some() {
                            ui.colored_label(egui::Color32::GREEN, "●");
                        }
                    });

                    if target.mode == OscMode::Raw {
                        ui.horizontal(|ui| {
                            ui.label("Address:");
                            ui.add(egui::TextEdit::singleline(&mut target.address).desired_width(100.0));
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.weak(egui::RichText::new("/lx/tempo/bpm, /lx/tempo/beat, /lx/tempo/beat-within-bar").small());
                        });
                    }
                });
                ui.add_space(4.0);
            }

            if let Some(idx) = to_remove {
                self.osc_targets.remove(idx);
                self.beat_phases.remove(idx);
                self.beat_in_bars.remove(idx);
                self.last_bpms.remove(idx);
            }

            if let Some(idx) = connect_idx {
                if let Some(target) = self.osc_targets.get_mut(idx) {
                    if let Err(e) = target.connect() {
                        self.error = Some(format!("OSC target {}: {}", idx + 1, e));
                    }
                }
            }

            if ui.button("+ Add OSC target").clicked() {
                self.osc_targets.push(OscTarget::raw());
                self.beat_phases.push(0.0);
                self.beat_in_bars.push(1);
                self.last_bpms.push(0.0);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // OSC Input
            ui.label("OSC Input:");
            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.osc_in_enabled, "Listen").changed() {
                    if self.osc_in_enabled {
                        self.start_osc_listener();
                    } else {
                        self.osc_in_listener = None;
                        self.osc_in_receiver = None;
                    }
                }
                if self.osc_in_listener.is_some() {
                    ui.colored_label(egui::Color32::GREEN, "●");
                }
            });
            ui.horizontal(|ui| {
                ui.label("Port:");
                ui.add(egui::TextEdit::singleline(&mut self.osc_in_port).desired_width(50.0));
                if ui.small_button("Restart").clicked() && self.osc_in_enabled {
                    self.start_osc_listener();
                }
            });
            ui.horizontal(|ui| {
                ui.weak(egui::RichText::new("/takt/reset  or  /bpm/reset → reset detector").small());
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}