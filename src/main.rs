#![windows_subsystem = "windows"]

mod palette;
mod panes;
mod profiles;
mod theme;

use std::sync::mpsc;

use alacritty_terminal::event::Event as PtyEvent;
use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Key, Modifiers, RichText, Stroke};
use egui_term::{
    BackendCommand, BackendSettings, FontSettings, TerminalBackend, TerminalFont, TerminalMode,
};
use serde::{Deserialize, Serialize};

use palette::{Command, Outcome, Palette};
use panes::{Axis, Direction, Node, Pane, PaneUi};
use profiles::Profile;
use theme::{Theme, SCHEMES};

const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 40.0;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const MAX_TAB_TITLE: usize = 28;

/// Preferences persisted between runs (eframe storage).
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    scheme: String,
    font_size: f32,
    copy_on_select: bool,
    confirm_multiline_paste: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scheme: SCHEMES[0].name.to_string(),
            font_size: DEFAULT_FONT_SIZE,
            copy_on_select: false,
            confirm_multiline_paste: true,
        }
    }
}

#[derive(Clone, Debug)]
enum Action {
    NewTab(usize),
    Split(Axis),
    ClosePane,
    CloseTab(usize),
    NextTab,
    PrevTab,
    GoToTab(usize),
    FocusPane(Direction),
    RenameTab(usize),
    Find,
    Copy,
    Paste,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleFullscreen,
    SetScheme(usize),
    ToggleCopyOnSelect,
    ToggleConfirmPaste,
    CommandPalette,
}

struct Tab {
    root: Option<Node>,
    focused: u64,
    profile: usize,
    /// Set by the user (double-click a tab); overrides the shell's title.
    custom_title: Option<String>,
    /// Output arrived while this tab was in the background.
    activity: bool,
    /// The shell rang the bell while this tab was in the background.
    bell: bool,
}

impl Tab {
    fn root(&self) -> &Node {
        self.root.as_ref().expect("tab without panes")
    }

    fn root_mut(&mut self) -> &mut Node {
        self.root.as_mut().expect("tab without panes")
    }

    fn focused_pane(&self) -> Option<&Pane> {
        self.root().pane(self.focused)
    }

    fn focused_backend(&mut self) -> Option<&mut TerminalBackend> {
        let id = self.focused;
        self.root_mut().pane_mut(id).map(|p| &mut p.backend)
    }

    fn title(&self) -> String {
        if let Some(t) = &self.custom_title {
            return t.clone();
        }
        self.focused_pane()
            .map(|p| p.title.clone())
            .unwrap_or_default()
    }
}

struct FindBar {
    query: String,
    has_focus: bool,
    request_focus: bool,
    no_match: bool,
}

struct MyTermApp {
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    pty_events: mpsc::Receiver<(u64, PtyEvent)>,
    pty_sender: mpsc::Sender<(u64, PtyEvent)>,

    profiles: Vec<Profile>,
    settings: Settings,
    theme: Theme,
    font: TerminalFont,

    palette: Option<Palette>,
    find: Option<FindBar>,
    renaming: Option<(usize, String)>,
    pending_paste: Option<(u64, String)>,
    toast: Option<(String, f64)>,
    pane_rects: Vec<(u64, egui::Rect)>,
    window_title: String,
}

impl MyTermApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        let theme = Theme::from_scheme(&SCHEMES[theme::scheme_index(&settings.scheme)]);
        cc.egui_ctx.set_visuals(theme.visuals());
        // We handle Ctrl +/-/0 ourselves to resize the terminal font only.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            pty_events: rx,
            pty_sender: tx,
            profiles: profiles::detect(),
            font: terminal_font(settings.font_size),
            settings,
            theme,
            palette: None,
            find: None,
            renaming: None,
            pending_paste: None,
            toast: None,
            pane_rects: Vec::new(),
            window_title: String::new(),
        };
        app.new_tab(&cc.egui_ctx, 0);
        app
    }

    // ---- tabs & panes -------------------------------------------------

    fn spawn_pane(&mut self, ctx: &egui::Context, profile: usize) -> Option<Pane> {
        let profile = &self.profiles[profile.min(self.profiles.len() - 1)];
        let id = self.next_id;
        self.next_id += 1;
        let settings = BackendSettings {
            shell: profile.shell.clone(),
            args: profile.args.clone(),
            working_directory: std::env::current_dir().ok(),
        };
        match TerminalBackend::new(id, ctx.clone(), self.pty_sender.clone(), settings) {
            Ok(backend) => Some(Pane {
                id,
                backend,
                title: profile.name.clone(),
            }),
            Err(e) => {
                self.show_toast(ctx, format!("Failed to start {}: {e}", profile.shell));
                None
            }
        }
    }

    fn new_tab(&mut self, ctx: &egui::Context, profile: usize) {
        if let Some(pane) = self.spawn_pane(ctx, profile) {
            self.tabs.push(Tab {
                focused: pane.id,
                root: Some(Node::Leaf(Box::new(pane))),
                profile,
                custom_title: None,
                activity: false,
                bell: false,
            });
            self.set_active(self.tabs.len() - 1);
        }
    }

    fn split(&mut self, ctx: &egui::Context, axis: Axis) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let (profile, target) = (tab.profile, tab.focused);
        let mut new = self.spawn_pane(ctx, profile);
        let Some(new_id) = new.as_ref().map(|p| p.id) else {
            return;
        };
        let tab = &mut self.tabs[self.active];
        tab.root = tab.root.take().map(|r| r.split(target, axis, &mut new));
        tab.focused = new_id;
    }

    fn set_active(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.activity = false;
            tab.bell = false;
            self.active = index;
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if index < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.renaming = None;
        let active = self.active;
        self.set_active(active);
    }

    /// Close one pane; the tab goes away with its last pane.
    fn close_pane(&mut self, pane_id: u64) {
        let Some(index) = self.tabs.iter().position(|t| t.root().contains(pane_id)) else {
            return;
        };
        let tab = &mut self.tabs[index];
        match tab.root.take().and_then(|r| r.remove(pane_id)) {
            Some(root) => {
                if tab.focused == pane_id {
                    tab.focused = root.first_id();
                }
                tab.root = Some(root);
            }
            None => self.close_tab(index),
        }
    }

    fn focused_pane_id(&self) -> Option<u64> {
        self.tabs.get(self.active).map(|t| t.focused)
    }

    fn backend_mut(&mut self, pane_id: u64) -> Option<&mut TerminalBackend> {
        self.tabs
            .iter_mut()
            .find_map(|t| t.root_mut().pane_mut(pane_id))
            .map(|p| &mut p.backend)
    }

    // ---- clipboard ----------------------------------------------------

    fn copy_selection(&mut self, ctx: &egui::Context, pane_id: u64) -> bool {
        let Some(backend) = self.backend_mut(pane_id) else {
            return false;
        };
        let text = backend.selectable_content();
        if text.is_empty() {
            return false;
        }
        ctx.copy_text(text);
        true
    }

    fn paste_from_clipboard(&mut self, ctx: &egui::Context, pane_id: u64) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => self.paste(pane_id, text),
            Err(_) => self.show_toast(ctx, "Clipboard has no text".into()),
        }
    }

    /// Paste into a pane, asking first for multi-line text (Windows Terminal's
    /// "multiLinePasteWarning") so a stray newline can't run a command.
    fn paste(&mut self, pane_id: u64, text: String) {
        if text.is_empty() {
            return;
        }
        if self.settings.confirm_multiline_paste && text.trim_end().contains('\n') {
            self.pending_paste = Some((pane_id, text));
        } else {
            self.write_paste(pane_id, &text);
        }
    }

    fn write_paste(&mut self, pane_id: u64, text: &str) {
        if let Some(backend) = self.backend_mut(pane_id) {
            let bracketed = backend
                .last_content()
                .terminal_mode
                .contains(TerminalMode::BRACKETED_PASTE);
            backend.process_command(BackendCommand::Write(paste_bytes(text, bracketed)));
        }
    }

    // ---- appearance ---------------------------------------------------

    fn set_scheme(&mut self, ctx: &egui::Context, index: usize) {
        let scheme = &SCHEMES[index];
        self.settings.scheme = scheme.name.to_string();
        self.theme = Theme::from_scheme(scheme);
        ctx.set_visuals(self.theme.visuals());
        self.show_toast(ctx, format!("Color scheme: {}", scheme.name));
    }

    fn set_font_size(&mut self, ctx: &egui::Context, size: f32) {
        self.settings.font_size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.font = terminal_font(self.settings.font_size);
        self.show_toast(ctx, format!("Font size {}", self.settings.font_size));
    }

    fn show_toast(&mut self, ctx: &egui::Context, text: String) {
        let until = ctx.input(|i| i.time) + 1.5;
        self.toast = Some((text, until));
    }

    // ---- actions ------------------------------------------------------

    fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::NewTab(profile) => self.new_tab(ctx, profile),
            Action::Split(axis) => self.split(ctx, axis),
            Action::ClosePane => {
                if let Some(id) = self.focused_pane_id() {
                    self.close_pane(id);
                }
            }
            Action::CloseTab(i) => self.close_tab(i),
            Action::NextTab if !self.tabs.is_empty() => {
                self.set_active((self.active + 1) % self.tabs.len());
            }
            Action::PrevTab if !self.tabs.is_empty() => {
                self.set_active((self.active + self.tabs.len() - 1) % self.tabs.len());
            }
            Action::GoToTab(i) => self.set_active(i),
            Action::FocusPane(dir) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if let Some(id) = panes::neighbor(&self.pane_rects, tab.focused, dir) {
                        tab.focused = id;
                    }
                }
            }
            Action::RenameTab(i) => {
                if let Some(tab) = self.tabs.get(i) {
                    self.renaming = Some((i, tab.title()));
                }
            }
            Action::Find => {
                if self.find.is_some() {
                    self.close_find();
                } else {
                    self.find = Some(FindBar {
                        query: String::new(),
                        has_focus: true,
                        request_focus: true,
                        no_match: false,
                    });
                }
            }
            Action::Copy => {
                if let Some(id) = self.focused_pane_id() {
                    if self.copy_selection(ctx, id) {
                        self.show_toast(ctx, "Copied".into());
                    }
                }
            }
            Action::Paste => {
                if let Some(id) = self.focused_pane_id() {
                    self.paste_from_clipboard(ctx, id);
                }
            }
            Action::ZoomIn => self.set_font_size(ctx, self.settings.font_size + 1.0),
            Action::ZoomOut => self.set_font_size(ctx, self.settings.font_size - 1.0),
            Action::ZoomReset => self.set_font_size(ctx, DEFAULT_FONT_SIZE),
            Action::ToggleFullscreen => {
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
            }
            Action::SetScheme(i) => self.set_scheme(ctx, i),
            Action::ToggleCopyOnSelect => {
                self.settings.copy_on_select = !self.settings.copy_on_select;
                let state = if self.settings.copy_on_select { "on" } else { "off" };
                self.show_toast(ctx, format!("Copy on select {state}"));
            }
            Action::ToggleConfirmPaste => {
                self.settings.confirm_multiline_paste = !self.settings.confirm_multiline_paste;
                let state = if self.settings.confirm_multiline_paste { "on" } else { "off" };
                self.show_toast(ctx, format!("Multi-line paste warning {state}"));
            }
            Action::CommandPalette => {
                self.palette = match self.palette {
                    Some(_) => None,
                    None => Some(Palette::new()),
                };
            }
            Action::NextTab | Action::PrevTab => {}
        }
    }

    fn close_find(&mut self) {
        self.find = None;
        if let Some(backend) = self.tabs.get_mut(self.active).and_then(|t| t.focused_backend()) {
            backend.process_command(BackendCommand::ClearSelection);
            backend.process_command(BackendCommand::ScrollToBottom);
        }
    }

    fn commands(&self) -> Vec<Command<Action>> {
        let cmd = |label: &str, shortcut: &'static str, action| Command {
            label: label.to_string(),
            shortcut,
            action,
        };
        let mut list = vec![cmd("New tab", "Ctrl+Shift+T", Action::NewTab(0))];
        const PROFILE_KEYS: [&str; 9] = [
            "Ctrl+Shift+1", "Ctrl+Shift+2", "Ctrl+Shift+3", "Ctrl+Shift+4", "Ctrl+Shift+5",
            "Ctrl+Shift+6", "Ctrl+Shift+7", "Ctrl+Shift+8", "Ctrl+Shift+9",
        ];
        for (i, p) in self.profiles.iter().enumerate() {
            let key = PROFILE_KEYS.get(i).copied().unwrap_or("");
            list.push(cmd(&format!("New tab: {}", p.name), key, Action::NewTab(i)));
        }
        list.extend([
            cmd("Split pane right", "Ctrl+Shift+E", Action::Split(Axis::Horizontal)),
            cmd("Split pane down", "Ctrl+Shift+O", Action::Split(Axis::Vertical)),
            cmd("Close pane", "Ctrl+Shift+W", Action::ClosePane),
            cmd("Close tab", "", Action::CloseTab(self.active)),
            cmd("Next tab", "Ctrl+Tab", Action::NextTab),
            cmd("Previous tab", "Ctrl+Shift+Tab", Action::PrevTab),
            cmd("Focus pane left", "Alt+Left", Action::FocusPane(Direction::Left)),
            cmd("Focus pane right", "Alt+Right", Action::FocusPane(Direction::Right)),
            cmd("Focus pane up", "Alt+Up", Action::FocusPane(Direction::Up)),
            cmd("Focus pane down", "Alt+Down", Action::FocusPane(Direction::Down)),
            cmd("Rename tab", "double-click tab", Action::RenameTab(self.active)),
            cmd("Find", "Ctrl+Shift+F", Action::Find),
            cmd("Copy selection", "Ctrl+Shift+C", Action::Copy),
            cmd("Paste", "Ctrl+Shift+V", Action::Paste),
            cmd("Font size: increase", "Ctrl+=", Action::ZoomIn),
            cmd("Font size: decrease", "Ctrl+-", Action::ZoomOut),
            cmd("Font size: reset", "Ctrl+0", Action::ZoomReset),
            cmd("Toggle full screen", "F11", Action::ToggleFullscreen),
            cmd(
                &format!("Copy on select: {}", on_off(!self.settings.copy_on_select)),
                "",
                Action::ToggleCopyOnSelect,
            ),
            cmd(
                &format!(
                    "Multi-line paste warning: {}",
                    on_off(!self.settings.confirm_multiline_paste)
                ),
                "",
                Action::ToggleConfirmPaste,
            ),
        ]);
        for (i, s) in SCHEMES.iter().enumerate() {
            list.push(cmd(&format!("Color scheme: {}", s.name), "", Action::SetScheme(i)));
        }
        list
    }

    // ---- input --------------------------------------------------------

    /// Pull our shortcuts out of the input queue *before* the terminal sees
    /// them, so e.g. Ctrl+Shift+T doesn't also send ^T to the shell.
    fn take_shortcuts(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = Vec::new();
        let ctrl = Modifiers::CTRL;
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;

        let mut bind = |hit: bool, action: Action| {
            if hit {
                actions.push(action);
            }
        };
        bind(take_key(ctx, ctrl_shift, Key::P), Action::CommandPalette);
        bind(take_key(ctx, ctrl_shift, Key::T), Action::NewTab(0));
        bind(take_key(ctx, ctrl_shift, Key::W), Action::ClosePane);
        bind(take_key(ctx, ctrl_shift, Key::E), Action::Split(Axis::Horizontal));
        bind(take_key(ctx, ctrl_shift, Key::O), Action::Split(Axis::Vertical));
        bind(take_key(ctx, ctrl_shift, Key::F), Action::Find);
        bind(take_key(ctx, ctrl_shift, Key::Tab), Action::PrevTab);
        bind(take_key(ctx, ctrl, Key::Tab), Action::NextTab);
        // Alt+Arrow only moves between panes when there are panes to move
        // between; otherwise the shell keeps it (word movement etc.).
        if self.tabs.get(self.active).is_some_and(|t| t.root().is_split()) {
            for (key, dir) in [
                (Key::ArrowLeft, Direction::Left),
                (Key::ArrowRight, Direction::Right),
                (Key::ArrowUp, Direction::Up),
                (Key::ArrowDown, Direction::Down),
            ] {
                bind(take_key(ctx, Modifiers::ALT, key), Action::FocusPane(dir));
            }
        }
        bind(
            take_key(ctx, ctrl, Key::Equals)
                || take_key(ctx, ctrl, Key::Plus)
                || take_key(ctx, ctrl_shift, Key::Plus)
                || take_key(ctx, ctrl_shift, Key::Equals),
            Action::ZoomIn,
        );
        bind(take_key(ctx, ctrl, Key::Minus), Action::ZoomOut);
        bind(take_physical(ctx, ctrl, Key::Num0), Action::ZoomReset);
        bind(
            take_key(ctx, Modifiers::NONE, Key::F11) || take_key(ctx, Modifiers::ALT, Key::Enter),
            Action::ToggleFullscreen,
        );
        const DIGITS: [Key; 9] = [
            Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5,
            Key::Num6, Key::Num7, Key::Num8, Key::Num9,
        ];
        for (i, key) in DIGITS.into_iter().enumerate() {
            bind(take_physical(ctx, ctrl_alt, key), Action::GoToTab(i));
            if i < self.profiles.len() {
                bind(take_physical(ctx, ctrl_shift, key), Action::NewTab(i));
            }
        }

        // Ctrl + mouse wheel resizes the font instead of scrolling.
        let wheel: f32 = ctx.input_mut(|i| {
            let mut total = 0.0;
            i.events.retain(|e| match e {
                egui::Event::MouseWheel {
                    delta, modifiers, ..
                } if modifiers.command => {
                    total += delta.y;
                    false
                }
                _ => true,
            });
            total
        });
        if wheel > 0.0 {
            actions.push(Action::ZoomIn);
        } else if wheel < 0.0 {
            actions.push(Action::ZoomOut);
        }

        // Ctrl+Shift+V / Shift+Insert: route through our paste so it gets
        // bracketed-paste handling and the multi-line warning. (Plain Ctrl+V
        // stays ^V for the shell, as in egui_term.)
        let pasted: Option<String> = ctx.input_mut(|i| {
            let shift = i.modifiers.shift;
            let mut text = None;
            i.events.retain(|e| match e {
                egui::Event::Paste(t) if shift => {
                    text = Some(t.clone());
                    false
                }
                _ => true,
            });
            text
        });
        if let (Some(text), Some(id)) = (pasted, self.focused_pane_id()) {
            if self.palette.is_none() && self.find.is_none() && self.renaming.is_none() {
                self.paste(id, text);
            }
        }

        actions
    }

    // ---- drawing ------------------------------------------------------

    fn draw_tab_bar(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let theme = &self.theme;
        let (tabbar, tab_active, accent, text_dim, fg) =
            (theme.tabbar, theme.tab_active, theme.accent, theme.text_dim, theme.fg);

        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::NONE
                    .fill(tabbar)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Menu & palette buttons stay pinned on the right.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("⌘").color(text_dim)).frame(false))
                            .on_hover_text("Command palette (Ctrl+Shift+P)")
                            .clicked()
                        {
                            actions.push(Action::CommandPalette);
                        }

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            egui::ScrollArea::horizontal()
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        self.draw_tabs(ui, actions, (tab_active, tabbar, accent, text_dim, fg));
                                        self.draw_new_tab_buttons(ui, actions, tabbar, text_dim);
                                    });
                                });
                        });
                    });
                });
            });
    }

    fn draw_tabs(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        (tab_active, tab_inactive, accent, text_dim, fg): (Color32, Color32, Color32, Color32, Color32),
    ) {
        let mut rename_done: Option<Option<String>> = None;

        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == self.active;
            let fill = if is_active { tab_active } else { tab_inactive };

            egui::Frame::NONE
                .fill(fill)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .stroke(if is_active {
                    Stroke::new(1.0_f32, accent)
                } else {
                    Stroke::NONE
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some((index, buffer)) = self.renaming.as_mut().filter(|(r, _)| *r == i) {
                            let _ = index;
                            let edit = ui.add(
                                egui::TextEdit::singleline(buffer)
                                    .desired_width(140.0)
                                    .font(FontId::proportional(13.0)),
                            );
                            edit.request_focus();
                            if ui.input(|inp| inp.key_pressed(Key::Escape)) {
                                rename_done = Some(None);
                            } else if edit.lost_focus() || ui.input(|inp| inp.key_pressed(Key::Enter)) {
                                rename_done = Some(Some(buffer.trim().to_string()));
                            }
                            return;
                        }

                        if tab.bell {
                            ui.label(RichText::new("🔔").color(accent));
                        } else if tab.activity {
                            ui.label(RichText::new("●").color(accent).small());
                        }
                        let full_title = tab.title();
                        let label_color = if is_active { fg } else { text_dim };
                        let resp = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(truncate_title(&full_title)).color(label_color),
                                )
                                .selectable(false)
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text(&full_title);
                        if resp.double_clicked() || resp.triple_clicked() {
                            actions.push(Action::RenameTab(i));
                        } else if resp.clicked() {
                            actions.push(Action::GoToTab(i));
                        }
                        if resp.middle_clicked() {
                            actions.push(Action::CloseTab(i));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Rename tab").clicked() {
                                actions.push(Action::RenameTab(i));
                                ui.close_menu();
                            }
                            if ui.button("Split pane right").clicked() {
                                actions.push(Action::GoToTab(i));
                                actions.push(Action::Split(Axis::Horizontal));
                                ui.close_menu();
                            }
                            if ui.button("Split pane down").clicked() {
                                actions.push(Action::GoToTab(i));
                                actions.push(Action::Split(Axis::Vertical));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Close tab").clicked() {
                                actions.push(Action::CloseTab(i));
                                ui.close_menu();
                            }
                        });
                        if ui
                            .add(
                                egui::Button::new(RichText::new("x").color(text_dim))
                                    .frame(false)
                                    .small(),
                            )
                            .clicked()
                        {
                            actions.push(Action::CloseTab(i));
                        }
                    });
                });

            ui.add_space(4.0);
        }

        if let Some(result) = rename_done {
            if let Some((index, _)) = self.renaming.take() {
                if let (Some(title), Some(tab)) = (result, self.tabs.get_mut(index)) {
                    tab.custom_title = (!title.is_empty()).then_some(title);
                }
            }
        }
    }

    fn draw_new_tab_buttons(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        tabbar: Color32,
        text_dim: Color32,
    ) {
        if ui
            .add(
                egui::Button::new(RichText::new("+").color(text_dim))
                    .fill(tabbar)
                    .frame(true)
                    .corner_radius(CornerRadius::same(8)),
            )
            .on_hover_text("New tab (Ctrl+Shift+T)")
            .clicked()
        {
            actions.push(Action::NewTab(0));
        }

        // Windows Terminal-style dropdown: profiles + appearance settings.
        ui.menu_button(RichText::new("⏷").color(text_dim), |ui| {
            for (i, p) in self.profiles.iter().enumerate() {
                let shortcut = if i < 9 { format!("Ctrl+Shift+{}", i + 1) } else { String::new() };
                if ui
                    .add(egui::Button::new(&p.name).shortcut_text(shortcut))
                    .clicked()
                {
                    actions.push(Action::NewTab(i));
                    ui.close_menu();
                }
            }
            ui.separator();
            ui.menu_button("Color scheme", |ui| {
                for (i, s) in SCHEMES.iter().enumerate() {
                    if ui
                        .radio(self.settings.scheme == s.name, s.name)
                        .clicked()
                    {
                        actions.push(Action::SetScheme(i));
                        ui.close_menu();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(format!("Font size {}", self.settings.font_size));
                if ui.small_button("−").clicked() {
                    actions.push(Action::ZoomOut);
                }
                if ui.small_button("+").clicked() {
                    actions.push(Action::ZoomIn);
                }
            });
            let mut copy_on_select = self.settings.copy_on_select;
            if ui.checkbox(&mut copy_on_select, "Copy on select").changed() {
                actions.push(Action::ToggleCopyOnSelect);
            }
            let mut confirm = self.settings.confirm_multiline_paste;
            if ui.checkbox(&mut confirm, "Warn on multi-line paste").changed() {
                actions.push(Action::ToggleConfirmPaste);
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Command palette…").shortcut_text("Ctrl+Shift+P"))
                .clicked()
            {
                actions.push(Action::CommandPalette);
                ui.close_menu();
            }
        });
    }

    fn draw_find_bar(&mut self, ctx: &egui::Context) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let (dim, error) = (self.theme.text_dim, Color32::from_rgb(0xff, 0x6b, 0x6b));
        let mut search: Option<bool> = None; // Some(upwards)
        let mut close = false;

        egui::Area::new(egui::Id::new("find_bar"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 52.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut find.query)
                                    .hint_text("Find")
                                    .desired_width(200.0)
                                    .text_color_opt(find.no_match.then_some(error)),
                            );
                            if find.request_focus {
                                edit.request_focus();
                                find.request_focus = false;
                            }
                            if edit.clicked() || edit.has_focus() {
                                find.has_focus = true;
                            }
                            if edit.changed() {
                                search = Some(true);
                            }
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                search = Some(!ui.input(|i| i.modifiers.shift));
                                find.request_focus = true;
                            }
                            if find.has_focus && ui.input(|i| i.key_pressed(Key::Escape)) {
                                close = true;
                            }
                            if ui.button("⏶").on_hover_text("Previous (Enter)").clicked() {
                                search = Some(true);
                            }
                            if ui.button("⏷").on_hover_text("Next (Shift+Enter)").clicked() {
                                search = Some(false);
                            }
                            if ui.add(egui::Button::new(RichText::new("x").color(dim)).frame(false)).clicked() {
                                close = true;
                            }
                        });
                    });
            });

        if close {
            self.close_find();
            return;
        }
        if let Some(upwards) = search {
            let query = find.query.clone();
            let changed_query = upwards && !find.request_focus;
            let found = match self.tabs.get_mut(self.active).and_then(|t| t.focused_backend()) {
                Some(backend) => {
                    if changed_query {
                        // A new query restarts from the bottom of the scrollback.
                        backend.process_command(BackendCommand::ClearSelection);
                    }
                    backend.search(&query, upwards)
                }
                None => false,
            };
            if let Some(find) = self.find.as_mut() {
                find.no_match = !query.is_empty() && !found;
            }
        }
    }

    fn draw_paste_confirm(&mut self, ctx: &egui::Context) {
        let Some((pane_id, text)) = self.pending_paste.as_ref() else {
            return;
        };
        let lines = text.trim_end().lines().count();
        let preview: String = text.lines().take(6).collect::<Vec<_>>().join("\n");
        let mut decision: Option<bool> = None;

        egui::Window::new("Paste multiple lines?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "You're about to paste {lines} lines. Each newline may run a command."
                ));
                ui.add_space(4.0);
                egui::Frame::NONE
                    .fill(self.theme.bg)
                    .inner_margin(egui::Margin::same(6))
                    .corner_radius(CornerRadius::same(4))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&preview).monospace().color(self.theme.text_dim));
                        if lines > 6 {
                            ui.label(RichText::new("…").color(self.theme.text_dim));
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Paste").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        decision = Some(false);
                    }
                });
            });

        if let Some(accept) = decision {
            let (pane_id, text) = (*pane_id, text.clone());
            self.pending_paste = None;
            if accept {
                self.write_paste(pane_id, &text);
            }
        }
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some((text, until)) = &self.toast else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now >= *until {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::new("toast"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(text.as_str());
                });
            });
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - now));
    }

    fn handle_pty_events(&mut self, ctx: &egui::Context) {
        while let Ok((id, event)) = self.pty_events.try_recv() {
            let Some(tab_idx) = self.tabs.iter().position(|t| t.root().contains(id)) else {
                continue;
            };
            let in_background = tab_idx != self.active;
            match event {
                PtyEvent::Wakeup => {
                    if in_background {
                        self.tabs[tab_idx].activity = true;
                    }
                }
                PtyEvent::Bell => {
                    if in_background {
                        self.tabs[tab_idx].bell = true;
                    }
                    if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                            egui::UserAttentionType::Informational,
                        ));
                    }
                }
                PtyEvent::Title(title) => {
                    if let Some(pane) = self.tabs[tab_idx].root_mut().pane_mut(id) {
                        pane.title = title;
                    }
                }
                PtyEvent::ResetTitle => {
                    let name = self.profiles[self.tabs[tab_idx].profile.min(self.profiles.len() - 1)]
                        .name
                        .clone();
                    if let Some(pane) = self.tabs[tab_idx].root_mut().pane_mut(id) {
                        pane.title = name;
                    }
                }
                // OSC 52: let programs (tmux, vim, ssh sessions…) set the clipboard.
                PtyEvent::ClipboardStore(_, text) => ctx.copy_text(text),
                PtyEvent::Exit | PtyEvent::ChildExit(_) => self.close_pane(id),
                _ => {}
            }
        }
    }
}

impl eframe::App for MyTermApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_pty_events(ctx);

        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut actions = self.take_shortcuts(ctx);
        // Checked before *and* after the tab bar: the Enter that finishes a
        // rename must not also reach the shell.
        let was_renaming = self.renaming.is_some();
        self.draw_tab_bar(ctx, &mut actions);

        // Overlays that own the keyboard keep the terminal from grabbing focus.
        let keyboard = self.palette.is_none()
            && !was_renaming
            && self.renaming.is_none()
            && self.pending_paste.is_none()
            && !self.find.as_ref().is_some_and(|f| f.has_focus);

        let theme = &self.theme;
        let tab = &mut self.tabs[self.active];
        let mut pane_ui = PaneUi {
            focused: tab.focused,
            keyboard,
            theme: &theme.terminal,
            font: &self.font,
            accent: theme.accent,
            divider: theme.divider,
            inactive_overlay: theme.inactive_overlay,
            split: tab.root().is_split(),
            pressed: None,
            right_clicked: None,
            selection_released: None,
            rects: Vec::new(),
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                tab.root_mut().show(ui, rect, &mut pane_ui);
            });

        let PaneUi {
            pressed,
            right_clicked,
            selection_released,
            rects,
            ..
        } = pane_ui;
        self.pane_rects = rects;
        if let Some(id) = pressed {
            self.tabs[self.active].focused = id;
            if let Some(find) = self.find.as_mut() {
                find.has_focus = false;
            }
        }
        // Right-click: copy the selection if there is one, otherwise paste
        // (Windows Terminal / conhost / PuTTY behavior).
        if let Some(id) = right_clicked {
            if self.copy_selection(ctx, id) {
                if let Some(backend) = self.backend_mut(id) {
                    backend.process_command(BackendCommand::ClearSelection);
                }
                self.show_toast(ctx, "Copied".into());
            } else {
                self.paste_from_clipboard(ctx, id);
            }
        }
        if let Some(id) = selection_released {
            if self.settings.copy_on_select && keyboard {
                self.copy_selection(ctx, id);
            }
        }

        self.draw_find_bar(ctx);
        if self.palette.is_some() {
            let commands = self.commands();
            let palette = self.palette.as_mut().unwrap();
            match palette.show(ctx, &commands, self.theme.accent, self.theme.text_dim) {
                Outcome::Open => {}
                Outcome::Close => self.palette = None,
                Outcome::Run(action) => {
                    self.palette = None;
                    actions.push(action);
                }
            }
        }
        self.draw_paste_confirm(ctx);
        self.draw_toast(ctx);

        for action in actions {
            self.run(ctx, action);
        }

        if let Some(tab) = self.tabs.get(self.active) {
            let title = format!("myterm — {}", tab.title());
            if title != self.window_title {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
                self.window_title = title;
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }
}

fn terminal_font(size: f32) -> TerminalFont {
    TerminalFont::new(FontSettings {
        font_type: FontId::monospace(size),
    })
}

fn on_off(on: bool) -> &'static str {
    if on {
        "turn on"
    } else {
        "turn off"
    }
}

/// Long shell titles (often a full exe path) keep their informative end.
fn truncate_title(title: &str) -> String {
    let count = title.chars().count();
    if count <= MAX_TAB_TITLE {
        return title.to_string();
    }
    let tail: String = title.chars().skip(count - (MAX_TAB_TITLE - 1)).collect();
    format!("…{tail}")
}

/// Bytes to send for a paste. In bracketed-paste mode the text is wrapped in
/// markers and stripped of escapes that could end the bracket early;
/// otherwise newlines become carriage returns, as a keyboard would send them.
fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let clean: String = text.chars().filter(|&c| c != '\x1b' && c != '\x03').collect();
        format!("\x1b[200~{clean}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// Remove a key press with exactly these modifiers from this frame's input.
fn take_key(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    take_key_by(ctx, modifiers, |k, _| k == key)
}

/// Like [`take_key`], but matches the physical key (for digits, whose logical
/// key changes with Shift and keyboard layout).
fn take_physical(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    take_key_by(ctx, modifiers, |k, physical| physical.unwrap_or(k) == key)
}

fn take_key_by(
    ctx: &egui::Context,
    modifiers: Modifiers,
    matches: impl Fn(Key, Option<Key>) -> bool,
) -> bool {
    ctx.input_mut(|i| {
        let mut hit = false;
        i.events.retain(|e| match e {
            egui::Event::Key {
                key,
                physical_key,
                pressed,
                modifiers: m,
                ..
            } if matches(*key, *physical_key) && m.matches_exact(modifiers) => {
                hit |= *pressed;
                false
            }
            _ => true,
        });
        hit
    })
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
        Box::new(|cc| Ok(Box::new(MyTermApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_plain_converts_newlines() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
    }

    #[test]
    fn paste_bracketed_wraps_and_strips_escapes() {
        assert_eq!(
            paste_bytes("x\x1b[201~y", true),
            b"\x1b[200~x[201~y\x1b[201~"
        );
    }

    #[test]
    fn long_titles_keep_their_end() {
        let t = truncate_title(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        assert!(t.starts_with('…'));
        assert!(t.ends_with("powershell.exe"));
        assert_eq!(t.chars().count(), MAX_TAB_TITLE);
        assert_eq!(truncate_title("pwsh"), "pwsh");
    }
}
