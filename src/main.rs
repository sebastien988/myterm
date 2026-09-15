// Release builds detach from the console so the terminal gets its own window
// and no stray console box. Debug builds keep it, so `println!`/panics during
// development still land somewhere visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod tabs;
mod theme;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::APP_NAME)
            .with_app_id(app::APP_NAME)
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([300.0, 200.0]),
        ..Default::default()
    };

    eframe::run_native(
        app::APP_NAME,
        native_options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme::BG;
            visuals.window_fill = theme::BG;
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(app::MyTermApp::new(cc)))
        }),
    )
}
