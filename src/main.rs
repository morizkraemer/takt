#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod detector;
mod link;
mod osc;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_title("bpm2"),
        ..Default::default()
    };
    eframe::run_native(
        "bpm2",
        options,
        Box::new(|_cc| Ok(Box::new(app::BpmApp::new()))),
    )
}
