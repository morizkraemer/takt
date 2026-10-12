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
            .with_title("takt")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .expect("valid embedded icon"),
            ),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "takt",
        options,
        Box::new(|cc| Ok(Box::new(app::BpmApp::new(cc.storage)))),
    )
}
