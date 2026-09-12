//! myterm — a small native terminal emulator with tabs.
//!
//! Rendering and input come from `egui`/`eframe`; each tab owns an
//! `egui_term::TerminalBackend`, which runs a shell on a PTY (ConPTY on
//! Windows) and parses its output with `alacritty_terminal`.
//!
//! * [`app`] holds the tabs, routes PTY events and drives the frame loop
//! * [`ui`] paints the tab strip, scrollback indicator and settings panel
//! * [`layout`] is the tab-strip and scrollbar arithmetic, kept testable
//! * [`input`] turns key presses into escape sequences or app actions
//! * [`config`] reads and writes `myterm.conf`
//! * [`theme`] defines the color schemes

#![windows_subsystem = "windows"]

mod app;
mod config;
mod input;
mod layout;
mod theme;
mod ui;

use eframe::egui;

use crate::app::MyTermApp;
use crate::config::Config;

fn main() -> eframe::Result<()> {
    let (config, config_path) = Config::load();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("myterm")
        .with_app_id("myterm")
        .with_inner_size([config.window_width, config.window_height])
        .with_min_inner_size([320.0, 200.0]);
    if config.window_maximized {
        viewport = viewport.with_maximized(true);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "myterm",
        native_options,
        Box::new(|cc| Ok(Box::new(MyTermApp::new(cc, config, config_path)))),
    )
}
