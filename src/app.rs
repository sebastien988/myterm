//! The application: a strip of tabs, each owning a PTY, over a terminal view.

use std::sync::mpsc;

use alacritty_terminal::event::{Event as PtyEvent, WindowSize};
use alacritty_terminal::term::ClipboardType;
use alacritty_terminal::vte::ansi::Rgb;
use eframe::egui;
use egui::{Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, Stroke};
use egui_term::{
    BackendCommand, BackendSettings, FontSettings, TerminalBackend, TerminalFont, TerminalTheme,
    TerminalView,
};

use crate::config::{self, Config};
use crate::tabs::{self, TabId, Tabs};
use crate::theme;

pub const APP_NAME: &str = "myterm";

const NEW_TAB: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::T);
const CLOSE_TAB: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::W);
const PREV_TAB: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Tab);
const NEXT_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);

/// A tab-strip interaction, applied once the panel closure has given back its
/// borrow of the tab list.
enum TabBarAction {
    Activate(usize),
    Close(usize),
    New,
}

pub struct MyTermApp {
    config: Config,
    terminal_theme: TerminalTheme,
    font: TerminalFont,
    tabs: Tabs<TerminalBackend>,
    pty_events: mpsc::Receiver<(TabId, PtyEvent)>,
    pty_sender: mpsc::Sender<(TabId, PtyEvent)>,
    /// Last failure to start a shell, surfaced in the window rather than on
    /// stderr: release builds detach from the console, so nothing printed
    /// there would ever be seen.
    error: Option<String>,
    /// What the window title currently says, so it is only set when it changes.
    window_title: Option<String>,
}

impl MyTermApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::from_env();
        let (pty_sender, pty_events) = mpsc::channel();

        let mut app = Self {
            terminal_theme: theme::terminal_theme(),
            font: TerminalFont::new(FontSettings {
                font_type: egui::FontId::monospace(config.font_size),
            }),
            config,
            tabs: Tabs::new(),
            pty_events,
            pty_sender,
            error: None,
            window_title: None,
        };
        app.spawn_tab(&cc.egui_ctx);
        app
    }

    fn spawn_tab(&mut self, ctx: &egui::Context) {
        let settings = BackendSettings {
            shell: self.config.shell.clone(),
            args: self.config.args.clone(),
            working_directory: self.config.working_directory.clone(),
        };
        let sender = self.pty_sender.clone();
        let ctx = ctx.clone();

        let spawned = self.tabs.push_with(tabs::DEFAULT_TITLE, move |id| {
            TerminalBackend::new(id, ctx, sender, settings)
        });

        self.error = match spawned {
            Ok(_) => None,
            Err(e) => Some(format!("Could not start `{}`: {e}", self.config.shell)),
        };
    }

    fn pump_pty_events(&mut self, ctx: &egui::Context) {
        while let Ok((id, event)) = self.pty_events.try_recv() {
            // Events can outlive their tab — a backend's shutdown is
            // asynchronous, so its last events arrive after it is gone.
            let Some(index) = self.tabs.index_of(id) else {
                continue;
            };

            match event {
                PtyEvent::Wakeup => {
                    if let Some(tab) = self.tabs.get_mut(index) {
                        tab.payload.sync();
                    }
                }
                PtyEvent::Title(title) => self.set_title(index, title),
                PtyEvent::ResetTitle => self.set_title(index, tabs::DEFAULT_TITLE.to_string()),
                // Replies the emulation layer owes the program on the other
                // end: device attributes, cursor position reports, mode
                // queries. Dropping these leaves whatever asked waiting for
                // its own timeout.
                PtyEvent::PtyWrite(text) => self.write_to_pty(index, text.into_bytes()),
                PtyEvent::TextAreaSizeRequest(format) => {
                    let size = self
                        .tabs
                        .get(index)
                        .map(|tab| WindowSize::from(tab.payload.last_content().terminal_size));
                    if let Some(size) = size {
                        self.write_to_pty(index, format(size).into_bytes());
                    }
                }
                PtyEvent::ColorRequest(color_index, format) => {
                    let color = theme::osc_color(&self.terminal_theme, color_index);
                    let reply = format(Rgb {
                        r: color.r(),
                        g: color.g(),
                        b: color.b(),
                    });
                    self.write_to_pty(index, reply.into_bytes());
                }
                // OSC 52. The primary selection has no equivalent here, so
                // only a clipboard request is honoured.
                PtyEvent::ClipboardStore(ClipboardType::Clipboard, text) => ctx.copy_text(text),
                PtyEvent::Exit | PtyEvent::ChildExit(_) => {
                    let _ = self.tabs.close(index);
                }
                _ => {}
            }
        }
    }

    fn set_title(&mut self, index: usize, title: String) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.title = title;
        }
    }

    fn write_to_pty(&mut self, index: usize, bytes: Vec<u8>) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.payload.process_command(BackendCommand::Write(bytes));
        }
    }

    /// Mirrors the active tab's title in the window title — including after a
    /// switch or a close, not only when a shell reports a new one.
    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let title = match self.tabs.active() {
            Some(tab) => format!(
                "{APP_NAME} — {}",
                tabs::display_title(&tab.title, tabs::MAX_TITLE_CHARS)
            ),
            None => APP_NAME.to_string(),
        };

        if self.window_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = Some(title);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Consumed rather than merely observed: egui_term binds Ctrl+Shift+T
        // and Ctrl+Shift+W to the control characters ^T and ^W, so a shortcut
        // left in the event queue also reaches the shell.
        let (new_tab, close_tab, prev_tab, next_tab) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&NEW_TAB),
                i.consume_shortcut(&CLOSE_TAB),
                // Ctrl+Shift+Tab is matched first: `matches_logically` ignores
                // an extra Shift, so the Ctrl+Tab pattern would otherwise
                // swallow it.
                i.consume_shortcut(&PREV_TAB),
                i.consume_shortcut(&NEXT_TAB),
            )
        });

        if new_tab {
            self.spawn_tab(ctx);
        }
        if close_tab {
            let index = self.tabs.active_index();
            let _ = self.tabs.close(index);
        }
        if prev_tab {
            self.tabs.select_prev();
        }
        if next_tab {
            self.tabs.select_next();
        }
    }

    fn draw_tab_bar(&mut self, ctx: &egui::Context) -> Option<TabBarAction> {
        let mut action = None;
        let active = self.tabs.active_index();
        let tabs = &self.tabs;

        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::NONE
                    .fill(theme::TABBAR_BG)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ctx, |ui| {
                // Many tabs scroll rather than pushing the `+` off the window.
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, tab) in tabs.iter().enumerate() {
                                let is_active = i == active;

                                egui::Frame::NONE
                                    .fill(if is_active {
                                        theme::TAB_ACTIVE
                                    } else {
                                        theme::TAB_INACTIVE
                                    })
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .stroke(if is_active {
                                        Stroke::new(1.0_f32, theme::ACCENT)
                                    } else {
                                        Stroke::NONE
                                    })
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let label = tabs::display_title(
                                                &tab.title,
                                                tabs::MAX_TITLE_CHARS,
                                            );
                                            let color = if is_active {
                                                Color32::WHITE
                                            } else {
                                                theme::TEXT_DIM
                                            };

                                            let clicked = ui
                                                .add(
                                                    egui::Label::new(
                                                        egui::RichText::new(label).color(color),
                                                    )
                                                    .sense(egui::Sense::click()),
                                                )
                                                .on_hover_text(tabs::display_title(
                                                    &tab.title,
                                                    usize::MAX,
                                                ))
                                                .clicked();
                                            if clicked {
                                                action = Some(TabBarAction::Activate(i));
                                            }

                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("×")
                                                            .color(theme::TEXT_DIM),
                                                    )
                                                    .frame(false)
                                                    .small(),
                                                )
                                                .on_hover_text("Close tab")
                                                .clicked()
                                            {
                                                action = Some(TabBarAction::Close(i));
                                            }
                                        });
                                    });

                                ui.add_space(4.0);
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("+").color(theme::TEXT_DIM),
                                    )
                                    .fill(theme::TABBAR_BG)
                                    .corner_radius(CornerRadius::same(8)),
                                )
                                .on_hover_text("New tab")
                                .clicked()
                            {
                                action = Some(TabBarAction::New);
                            }
                        });
                    });
            });

        action
    }

    fn draw_error_banner(&mut self, ctx: &egui::Context) {
        let Some(message) = self.error.clone() else {
            return;
        };
        let mut dismissed = false;

        egui::TopBottomPanel::top("error_banner")
            .frame(
                egui::Frame::NONE
                    .fill(theme::TABBAR_BG)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(message.as_str()).color(theme::ERROR));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        dismissed = ui.button("Dismiss").clicked();
                    });
                });
            });

        if dismissed {
            self.error = None;
        }
    }

    /// Shown when the shell could not be started at all. Without it a failure
    /// at startup would leave no tabs, which closes the window instantly and
    /// looks like the app refusing to launch for no reason.
    fn draw_startup_error(&mut self, ctx: &egui::Context) {
        let message = self.error.clone().unwrap_or_default();
        let mut retry = false;
        let mut quit = false;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(24)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    ui.label(
                        egui::RichText::new("No shell running")
                            .color(theme::ERROR)
                            .size(20.0),
                    );
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(message.as_str()).color(theme::TEXT_DIM));
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Set {} to launch a different shell.",
                            config::SHELL_ENV
                        ))
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        retry = ui.button("Try again").clicked();
                        quit = ui.button("Quit").clicked();
                    });
                });
            });

        if retry {
            self.spawn_tab(ctx);
        }
        if quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn draw_terminal(&mut self, ctx: &egui::Context) {
        let terminal_theme = self.terminal_theme.clone();
        let font = self.font.clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ctx, |ui| {
                if let Some(tab) = self.tabs.active_mut() {
                    let view = TerminalView::new(ui, &mut tab.payload)
                        .set_focus(true)
                        .set_theme(terminal_theme)
                        .set_font(font);
                    ui.add(view);
                }
            });
    }
}

impl eframe::App for MyTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_pty_events(ctx);

        if self.tabs.is_empty() {
            self.sync_window_title(ctx);
            if self.error.is_some() {
                self.draw_startup_error(ctx);
            } else {
                // The last shell exited; the window goes with it.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }

        self.handle_shortcuts(ctx);

        if let Some(action) = self.draw_tab_bar(ctx) {
            match action {
                TabBarAction::Activate(index) => self.tabs.select(index),
                TabBarAction::Close(index) => {
                    let _ = self.tabs.close(index);
                }
                TabBarAction::New => self.spawn_tab(ctx),
            }
        }

        self.sync_window_title(ctx);
        self.draw_error_banner(ctx);
        self.draw_terminal(ctx);
    }
}
