//! All of myterm's own widgets: the tab strip, the scrollback indicator, the
//! settings panel and the status overlays.
//!
//! The tab strip is painted by hand rather than assembled from egui buttons so
//! that a tab can shrink gracefully, elide its title, and keep the close
//! button, the activity dot and the accent underline in fixed positions.

use alacritty_terminal::grid::Dimensions;
use eframe::egui;
use egui::{
    Align, Align2, Color32, Context, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, Vec2,
};
use egui_term::TerminalView;

use crate::app::MyTermApp;
use crate::config;
use crate::input::SHORTCUTS;
use crate::theme;

const TAB_HEIGHT: f32 = 26.0;
const TAB_GAP: f32 = 4.0;
const TAB_MAX_WIDTH: f32 = 220.0;
const TAB_MIN_WIDTH: f32 = 28.0;
const ICON_SIZE: f32 = 26.0;
const SCROLLBAR_WIDTH: f32 = 10.0;

enum TabBarAction {
    Activate(usize),
    Close(usize),
    NewTab,
    NewFrom(usize),
    ToggleSettings,
}

impl MyTermApp {
    pub(crate) fn draw(&mut self, ctx: &Context) {
        if self.fatal.is_some() {
            self.draw_fatal(ctx);
            self.draw_notice(ctx);
            return;
        }

        if !(self.config.tab_bar_auto_hide && self.tabs.len() <= 1) {
            self.draw_tab_bar(ctx);
        }
        if self.config.scrollbar {
            self.draw_scrollbar(ctx);
        }
        self.draw_terminal(ctx);
        self.draw_settings(ctx);
        self.draw_close_confirm(ctx);
        self.draw_notice(ctx);
    }

    fn draw_tab_bar(&mut self, ctx: &Context) {
        let chrome = self.chrome.clone();
        let mut action: Option<TabBarAction> = None;
        let profiles = self.config.menu_profiles();
        let font = FontId::proportional(13.0);

        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::NONE
                    .fill(chrome.tabbar_bg)
                    .inner_margin(Margin {
                        left: 6,
                        right: 6,
                        top: 5,
                        bottom: 5,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Space kept clear on the right for the new-tab and
                    // settings buttons.
                    let reserved = ICON_SIZE * 2.0 + TAB_GAP * 3.0;
                    let count = self.tabs.len().max(1) as f32;
                    let available = (ui.available_width() - reserved).max(TAB_MIN_WIDTH);
                    let width = ((available - TAB_GAP * (count - 1.0)) / count)
                        .clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);

                    for index in 0..self.tabs.len() {
                        if index > 0 {
                            ui.add_space(TAB_GAP);
                        }
                        if let Some(hit) = self.draw_tab(ui, index, width, &font) {
                            action = Some(hit);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, "⚙", &chrome)
                            .on_hover_text("Settings (Ctrl+,)")
                            .clicked()
                        {
                            action = Some(TabBarAction::ToggleSettings);
                        }
                        ui.add_space(TAB_GAP);
                        let plus = icon_button(ui, "+", &chrome)
                            .on_hover_text("New tab (Ctrl+Shift+T) — right-click to pick a shell");
                        if plus.clicked() {
                            action = Some(TabBarAction::NewTab);
                        }
                        if profiles.len() > 1 {
                            plus.context_menu(|ui| {
                                for (index, profile) in profiles.iter().enumerate() {
                                    if ui.button(&profile.name).clicked() {
                                        action = Some(TabBarAction::NewFrom(index));
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                    });

                    // A wheel over the strip walks through the tabs.
                    if ui.ui_contains_pointer() && self.tabs.len() > 1 {
                        let scroll = ui.input(|i| i.raw_scroll_delta.y);
                        if scroll.abs() >= 1.0 {
                            let delta = if scroll > 0.0 { -1 } else { 1 };
                            let next = (self.active as i64 + delta as i64)
                                .rem_euclid(self.tabs.len() as i64)
                                as usize;
                            action = Some(TabBarAction::Activate(next));
                        }
                    }
                });
            });

        match action {
            Some(TabBarAction::Activate(index)) => self.activate(ctx, index),
            Some(TabBarAction::Close(index)) => self.close_tab(ctx, index),
            Some(TabBarAction::NewTab) => {
                let profile = self.config.default_profile();
                self.spawn_tab(ctx, &profile);
            }
            Some(TabBarAction::NewFrom(index)) => {
                if let Some(profile) = profiles.get(index).cloned() {
                    self.spawn_tab(ctx, &profile);
                }
            }
            Some(TabBarAction::ToggleSettings) => self.show_settings = !self.show_settings,
            None => {}
        }
    }

    fn draw_tab(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        width: f32,
        font: &FontId,
    ) -> Option<TabBarAction> {
        let chrome = &self.chrome;
        let tab = &self.tabs[index];
        let is_active = index == self.active;

        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, TAB_HEIGHT), Sense::click());
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let hovered = response.hovered();

        let fill = if is_active {
            chrome.tab_active
        } else if hovered {
            chrome.tab_hover
        } else {
            chrome.tab_inactive
        };

        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(7), fill);
        if is_active {
            painter.rect_stroke(
                rect,
                CornerRadius::same(7),
                Stroke::new(1.0_f32, chrome.accent),
                StrokeKind::Inside,
            );
            // A short accent underline, the way Konsole marks the active tab.
            let underline = Rect::from_min_max(
                Pos2::new(rect.left() + 8.0, rect.bottom() - 3.0),
                Pos2::new(rect.right() - 8.0, rect.bottom() - 1.5),
            );
            painter.rect_filled(underline, CornerRadius::same(1), chrome.accent);
        }

        let mut text_left = rect.left() + 9.0;
        if !is_active && (tab.unread || tab.bell) {
            let color = if tab.bell { chrome.warn } else { chrome.accent };
            painter.circle_filled(Pos2::new(text_left + 3.0, rect.center().y), 3.0, color);
            text_left += 12.0;
        }

        // The close button only appears when it can be hit comfortably.
        let show_close = width > 70.0 && (is_active || hovered);
        let close_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 13.0, rect.center().y),
            Vec2::splat(16.0),
        );
        let close_hovered = show_close && pointer.is_some_and(|pos| close_rect.contains(pos));

        let text_right = if show_close {
            close_rect.left() - 4.0
        } else {
            rect.right() - 8.0
        };
        let text_color = if is_active {
            chrome.text
        } else {
            chrome.text_dim
        };
        if text_right > text_left {
            let galley = elided_text(
                ui,
                tab.display_title(),
                font.clone(),
                text_color,
                text_right - text_left,
            );
            let pos = Pos2::new(text_left, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(pos, galley, text_color);
        }

        if show_close {
            if close_hovered {
                ui.painter()
                    .circle_filled(close_rect.center(), 8.0, chrome.tab_hover);
            }
            ui.painter().text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "×",
                FontId::proportional(14.0),
                if close_hovered {
                    chrome.text
                } else {
                    chrome.text_dim
                },
            );
        }

        if tab.exited.is_some() {
            response.clone().on_hover_text(format!(
                "{} exited with code {}",
                tab.label,
                tab.exited.unwrap_or_default()
            ));
        }

        let clicked_close = close_hovered
            && response
                .interact_pointer_pos()
                .is_some_and(|pos| close_rect.contains(pos));

        if response.clicked_by(egui::PointerButton::Middle) || (response.clicked() && clicked_close)
        {
            Some(TabBarAction::Close(index))
        } else if response.clicked() {
            Some(TabBarAction::Activate(index))
        } else {
            None
        }
    }

    /// A slim scrollback indicator on the right edge. It lives in its own
    /// panel so dragging it never starts a text selection in the grid.
    fn draw_scrollbar(&mut self, ctx: &Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let grid = &tab.backend.last_content().grid;
        let history = grid.history_size();
        let offset = grid.display_offset();
        let screen = grid.screen_lines();
        let chrome = self.chrome.clone();
        let background = self.palette.background;
        let mut scroll_to: Option<usize> = None;

        egui::SidePanel::right("scrollback")
            .exact_width(SCROLLBAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(background))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if history == 0 {
                    return;
                }

                let track = rect.shrink2(Vec2::new(2.5, 3.0));
                let total = (history + screen) as f32;
                let thumb_height =
                    (track.height() * screen as f32 / total).clamp(24.0, track.height());
                let travel = track.height() - thumb_height;
                // display_offset counts lines above the viewport: 0 is the
                // bottom of the scrollback.
                let position = 1.0 - offset as f32 / history as f32;
                let top = track.top() + travel * position;

                let scrolled_back = offset > 0;
                let color = if scrolled_back || response.hovered() {
                    chrome.accent
                } else {
                    chrome.border
                };
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(track.left(), top),
                        Vec2::new(track.width(), thumb_height),
                    ),
                    CornerRadius::same(2),
                    color,
                );

                if let Some(pos) = response.interact_pointer_pos() {
                    if response.dragged() || response.clicked() {
                        let wanted = ((pos.y - track.top() - thumb_height / 2.0) / travel.max(1.0))
                            .clamp(0.0, 1.0);
                        scroll_to = Some(((1.0 - wanted) * history as f32).round() as usize);
                    }
                }
            });

        if let Some(target) = scroll_to {
            let delta = target as i32 - offset as i32;
            if delta != 0 {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.backend
                        .process_command(egui_term::BackendCommand::Scroll(delta));
                }
            }
        }
    }

    fn draw_terminal(&mut self, ctx: &Context) {
        let font = self.terminal_font();
        let theme = self.term_theme.clone();
        let chrome = self.chrome.clone();
        let background = self.palette.background;
        let active = self.active;
        let exited = self.tabs.get(active).and_then(|tab| tab.exited);
        let mut dismiss = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(background).inner_margin(Margin {
                left: 6,
                right: 0,
                top: 4,
                bottom: 0,
            }))
            .show(ctx, |ui| {
                let Some(tab) = self.tabs.get_mut(active) else {
                    return;
                };
                let view = TerminalView::new(ui, &mut tab.backend)
                    .set_theme(theme)
                    .set_font(font)
                    .set_focus(true);
                let response = ui.add(view);

                if let Some(code) = exited {
                    let rect = response.rect;
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        background.gamma_multiply(0.75),
                    );
                    let center = rect.center();
                    ui.painter().text(
                        center - Vec2::new(0.0, 10.0),
                        Align2::CENTER_CENTER,
                        format!("The shell exited with code {code}"),
                        FontId::proportional(16.0),
                        chrome.text,
                    );
                    ui.painter().text(
                        center + Vec2::new(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        "Press Enter or click to close this tab",
                        FontId::proportional(13.0),
                        chrome.text_dim,
                    );
                    if response.clicked() {
                        dismiss = true;
                    }
                }
            });

        if dismiss {
            self.close_tab(ctx, active);
        }
    }

    fn draw_settings(&mut self, ctx: &Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut reload = false;
        let path_label = self
            .config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<no config directory>".to_string());

        egui::Window::new("myterm settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .anchor(Align2::RIGHT_TOP, Vec2::new(-12.0, 44.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                ui.label(egui::RichText::new("Appearance").strong());
                ui.horizontal_wrapped(|ui| {
                    for palette in theme::PALETTES {
                        let selected = palette.name == self.palette.name;
                        if ui.selectable_label(selected, palette.name).clicked() && !selected {
                            self.set_palette(ui.ctx(), palette);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Font size");
                    if ui.button("−").on_hover_text("Ctrl+-").clicked() {
                        self.set_font_size(self.font_size - 1.0);
                    }
                    ui.label(format!("{:.0} pt", self.font_size));
                    if ui.button("+").on_hover_text("Ctrl+=").clicked() {
                        self.set_font_size(self.font_size + 1.0);
                    }
                    if ui.button("Reset").clicked() {
                        self.set_font_size(config::DEFAULT_FONT_SIZE);
                    }
                });

                ui.separator();
                ui.label(egui::RichText::new("Behaviour").strong());
                let mut auto_hide = self.config.tab_bar_auto_hide;
                if ui
                    .checkbox(&mut auto_hide, "Hide the tab bar with a single tab")
                    .changed()
                {
                    self.config.tab_bar_auto_hide = auto_hide;
                }
                let mut confirm = self.config.confirm_close;
                if ui
                    .checkbox(&mut confirm, "Confirm closing a window with several tabs")
                    .changed()
                {
                    self.config.confirm_close = confirm;
                }
                let mut scrollbar = self.config.scrollbar;
                if ui
                    .checkbox(&mut scrollbar, "Show the scrollback indicator")
                    .changed()
                {
                    self.config.scrollbar = scrollbar;
                }

                ui.separator();
                ui.label(egui::RichText::new("Shells").strong());
                for profile in self.config.menu_profiles() {
                    let command = if profile.args.is_empty() {
                        profile.command.clone()
                    } else {
                        format!("{} {}", profile.command, profile.args.join(" "))
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", profile.name));
                        ui.weak(command);
                    });
                }
                ui.weak("Add more with `profile.<name> = <command>` in the config file.");

                ui.separator();
                ui.collapsing("Keyboard shortcuts", |ui| {
                    egui::Grid::new("shortcuts").num_columns(2).show(ui, |ui| {
                        for (keys, what) in SHORTCUTS {
                            ui.label(egui::RichText::new(*keys).monospace());
                            ui.weak(*what);
                            ui.end_row();
                        }
                    });
                });

                ui.separator();
                ui.horizontal(|ui| {
                    save = ui
                        .button("Save to config")
                        .on_hover_text(&path_label)
                        .clicked();
                    reload = ui.button("Reload config").clicked();
                    if ui.button("Copy config path").clicked() {
                        ui.ctx().copy_text(path_label.clone());
                    }
                });
                ui.weak(format!("myterm {}", env!("CARGO_PKG_VERSION")));
            });

        if save && self.save_config() {
            self.note(format!("settings written to {path_label}"));
        }
        if reload {
            self.reload_config(ctx);
        }
        if !open {
            self.show_settings = false;
        }
    }

    fn draw_close_confirm(&mut self, ctx: &Context) {
        if !self.show_close_confirm {
            return;
        }
        let count = self.tabs.len();
        let mut close = false;
        let mut cancel = false;

        egui::Modal::new(Id::new("confirm_close")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.heading("Close myterm?");
            ui.add_space(6.0);
            ui.label(format!("{count} tabs are still open."));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                close = ui.button("Close all tabs").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });

        if close {
            self.confirm_close(ctx);
        } else if cancel {
            self.show_close_confirm = false;
        }
    }

    /// The screen shown when there is no shell to display, e.g. because the
    /// configured one could not be started.
    fn draw_fatal(&mut self, ctx: &Context) {
        let message = self.fatal.clone().unwrap_or_default();
        let chrome = self.chrome.clone();
        let path = self
            .config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut retry = false;
        let mut settings = false;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(self.palette.background)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new("myterm could not start a shell")
                            .size(18.0)
                            .color(chrome.text),
                    );
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(&message).color(chrome.warn));
                    ui.add_space(10.0);
                    if !path.is_empty() {
                        ui.weak(format!("Set `shell` in {path}"));
                    }
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        retry = ui.button("Try again").clicked();
                        settings = ui.button("Settings").clicked();
                        if ui.button("Quit").clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        if settings {
            self.show_settings = !self.show_settings;
        }
        if retry {
            let profile = self.config.default_profile();
            self.spawn_tab(ctx, &profile);
        }
        self.draw_settings(ctx);
    }

    fn draw_notice(&mut self, ctx: &Context) {
        let Some(notice) = &self.notice else {
            return;
        };
        let chrome = self.chrome.clone();
        let color = if notice.is_error {
            chrome.warn
        } else {
            chrome.text
        };
        let text = notice.text.clone();
        let mut dismiss = false;

        egui::Area::new(Id::new("notice"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-14.0, -14.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(chrome.tabbar_bg)
                    .stroke(Stroke::new(1.0_f32, chrome.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(text).color(color));
                            if ui.small_button("×").clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });

        if dismiss {
            self.notice = None;
        }
    }
}

/// Lay out `text` on a single line, eliding it with `…` when too wide.
fn elided_text(
    ui: &egui::Ui,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id,
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// A small square button painted in the chrome colors.
fn icon_button(ui: &mut egui::Ui, glyph: &str, chrome: &theme::Chrome) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_SIZE), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), chrome.tab_hover);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(15.0),
        if response.hovered() {
            chrome.text
        } else {
            chrome.text_dim
        },
    );
    response
}
