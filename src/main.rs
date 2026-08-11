#![windows_subsystem = "windows"]

use std::sync::mpsc;

use alacritty_terminal::event::Event as PtyEvent;
use eframe::egui;
use egui::{Color32, CornerRadius, Stroke};
use egui_term::{BackendSettings, TerminalBackend, TerminalView};

const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x16);
const TABBAR_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x20);
const TAB_ACTIVE: Color32 = Color32::from_rgb(0x24, 0x24, 0x2e);
const TAB_INACTIVE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x20);
const ACCENT: Color32 = Color32::from_rgb(0xa0, 0x6c, 0xff);
const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xab);

fn default_shell() -> String {
    std::env::var("MYTERM_SHELL").unwrap_or_else(|_| "powershell.exe".to_string())
}

struct Tab {
    id: u64,
    backend: TerminalBackend,
    title: String,
}

struct MyTermApp {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    pty_events: mpsc::Receiver<(u64, PtyEvent)>,
    pty_sender: mpsc::Sender<(u64, PtyEvent)>,
}

impl MyTermApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            pty_events: rx,
            pty_sender: tx,
        };
        app.spawn_tab(&cc.egui_ctx);
        app
    }

    fn spawn_tab(&mut self, ctx: &egui::Context) {
        let id = self.next_id;
        self.next_id += 1;
        let settings = BackendSettings {
            shell: default_shell(),
            args: vec![],
            working_directory: std::env::current_dir().ok(),
        };
        match TerminalBackend::new(id, ctx.clone(), self.pty_sender.clone(), settings) {
            Ok(backend) => {
                self.tabs.push(Tab {
                    id,
                    backend,
                    title: "shell".to_string(),
                });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => {
                eprintln!("failed to spawn shell: {e}");
            }
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if self.active >= self.tabs.len() && !self.tabs.is_empty() {
            self.active = self.tabs.len() - 1;
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (new_tab, close_tab, next_tab, prev_tab) = ctx.input(|i| {
            (
                i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::T),
                i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::W),
                i.modifiers.ctrl && i.key_pressed(egui::Key::Tab) && !i.modifiers.shift,
                i.modifiers.ctrl && i.key_pressed(egui::Key::Tab) && i.modifiers.shift,
            )
        });

        if new_tab {
            self.spawn_tab(ctx);
        }
        if close_tab && !self.tabs.is_empty() {
            self.close_tab(self.active);
        }
        if next_tab && !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if prev_tab && !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    fn draw_tab_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::NONE
                    .fill(TABBAR_BG)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut close_requested: Option<usize> = None;
                    let mut activate_requested: Option<usize> = None;

                    for (i, tab) in self.tabs.iter().enumerate() {
                        let is_active = i == self.active;
                        let fill = if is_active { TAB_ACTIVE } else { TAB_INACTIVE };

                        egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .stroke(if is_active {
                                Stroke::new(1.0_f32, ACCENT)
                            } else {
                                Stroke::NONE
                            })
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label_color =
                                        if is_active { Color32::WHITE } else { TEXT_DIM };
                                    let resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!(
                                                "  {}",
                                                tab.title
                                            ))
                                            .color(label_color),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if resp.clicked() {
                                        activate_requested = Some(i);
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("x").color(TEXT_DIM),
                                            )
                                            .frame(false)
                                            .small(),
                                        )
                                        .clicked()
                                    {
                                        close_requested = Some(i);
                                    }
                                });
                            });

                        ui.add_space(4.0);
                    }

                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("+").color(TEXT_DIM))
                                .fill(TABBAR_BG)
                                .frame(true)
                                .corner_radius(CornerRadius::same(8)),
                        )
                        .clicked()
                    {
                        self.spawn_tab(ctx);
                    }

                    if let Some(i) = activate_requested {
                        self.active = i;
                    }
                    if let Some(i) = close_requested {
                        self.close_tab(i);
                    }
                });
            });
    }
}

impl eframe::App for MyTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok((id, event)) = self.pty_events.try_recv() {
            let Some(tab_idx) = self.tabs.iter().position(|t| t.id == id) else {
                continue;
            };
            match event {
                PtyEvent::Wakeup => {
                    self.tabs[tab_idx].backend.sync();
                }
                PtyEvent::Title(title) => {
                    self.tabs[tab_idx].title = title.clone();
                    if tab_idx == self.active {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                            "myterm — {title}"
                        )));
                    }
                }
                PtyEvent::Exit | PtyEvent::ChildExit(_) => {
                    self.close_tab(tab_idx);
                }
                _ => {}
            }
        }

        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.handle_shortcuts(ctx);
        self.draw_tab_bar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG))
            .show(ctx, |ui| {
                let active = self.active;
                let backend = &mut self.tabs[active].backend;
                let view = TerminalView::new(ui, backend).set_focus(true);
                ui.add(view);
            });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("myterm")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([300.0, 200.0]),
        ..Default::default()
    };

    eframe::run_native(
        "myterm",
        native_options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = BG;
            visuals.window_fill = BG;
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(MyTermApp::new(cc)))
        }),
    )
}
