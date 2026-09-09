//! Application state: tabs, PTY event routing, and the update loop.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as PtyEvent, WindowSize};
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::Rgb;
use eframe::egui;
use egui::{Context, FontFamily, FontId, Vec2, ViewportCommand};
use egui_term::{
    BackendCommand, BackendSettings, FontSettings, TerminalBackend, TerminalFont, TerminalMode,
    TerminalTheme,
};

use crate::config::{self, Config, Profile};
use crate::input::{self, Action};
use crate::theme::{self, Chrome, Palette};

/// How long a status message stays on screen.
const NOTICE_TIMEOUT: Duration = Duration::from_secs(8);

pub struct Tab {
    pub id: u64,
    pub backend: TerminalBackend,
    /// Title reported by the shell, if any.
    pub title: Option<String>,
    /// Fallback label (the profile name), used until the shell sets a title.
    pub label: String,
    /// The shell wrote something while this tab was in the background.
    pub unread: bool,
    /// The shell rang the bell while this tab was in the background.
    pub bell: bool,
    /// Set when the shell exited with a non-zero status; the tab is kept
    /// around so the error is readable instead of the window vanishing.
    pub exited: Option<i32>,
}

impl Tab {
    pub fn display_title(&self) -> &str {
        match &self.title {
            Some(title) if !title.trim().is_empty() => title,
            _ => &self.label,
        }
    }
}

pub struct Notice {
    pub text: String,
    pub is_error: bool,
    pub shown_at: Instant,
}

pub struct MyTermApp {
    pub(crate) config: Config,
    pub(crate) config_path: Option<PathBuf>,
    /// Last state written to disk, so we only rewrite the file when needed.
    pub(crate) saved_config: Config,
    pub(crate) palette: &'static Palette,
    pub(crate) chrome: Chrome,
    pub(crate) term_theme: TerminalTheme,
    pub(crate) font_size: f32,

    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    next_id: u64,
    pty_events: mpsc::Receiver<(u64, PtyEvent)>,
    pty_sender: mpsc::Sender<(u64, PtyEvent)>,

    pub(crate) show_settings: bool,
    pub(crate) show_close_confirm: bool,
    pub(crate) notice: Option<Notice>,
    /// Set when there is nothing left to show: no tabs and a reason why.
    pub(crate) fatal: Option<String>,
    fullscreen: bool,
    closing: bool,
    zoom_accumulator: f32,
}

/// The selected text, as a terminal user expects to paste it elsewhere.
///
/// `egui_term::TerminalBackend::selectable_content` concatenates every
/// selected cell, so a multi-line selection arrives as one long line padded
/// with the blanks at the end of each row. This walks the same cells but keeps
/// the line structure: rows are separated by newlines unless the row wrapped,
/// and trailing blanks are dropped.
fn selection_text(grid: &Grid<Cell>, range: Option<SelectionRange>) -> String {
    let Some(range) = range else {
        return String::new();
    };
    let last_column = Column(grid.columns().saturating_sub(1));
    let mut text = String::new();
    let mut current_line: Option<Line> = None;
    let mut pending_blanks = 0usize;

    for indexed in grid.display_iter() {
        if !range.contains(indexed.point) || indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        if current_line != Some(indexed.point.line) {
            if let Some(previous) = current_line {
                let wrapped = !range.is_block
                    && grid[Point::new(previous, last_column)]
                        .flags
                        .contains(Flags::WRAPLINE);
                if !wrapped {
                    text.push('\n');
                }
            }
            current_line = Some(indexed.point.line);
            pending_blanks = 0;
        }

        if indexed.c == ' ' {
            pending_blanks += 1;
        } else {
            for _ in 0..pending_blanks {
                text.push(' ');
            }
            pending_blanks = 0;
            text.push(indexed.c);
        }
    }

    text
}

/// The tab that should be active after removing `closed`.
fn active_after_close(len_before: usize, closed: usize, active: usize) -> usize {
    if len_before <= 1 {
        return 0;
    }
    let len_after = len_before - 1;
    if closed < active {
        active - 1
    } else {
        active.min(len_after - 1)
    }
}

/// Where a tab lands when moved by `delta`, wrapping around the strip.
fn moved_index(len: usize, from: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let len_i = len as i64;
    (((from as i64 + delta as i64) % len_i + len_i) % len_i) as usize
}

impl MyTermApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        config_path: Option<PathBuf>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let palette = theme::palette_by_name(&config.theme).unwrap_or_else(theme::default_palette);
        let font_size = config.font_size;
        let warnings = config.warnings.clone();

        let mut app = Self {
            saved_config: config.clone(),
            config,
            config_path,
            palette,
            chrome: Chrome::from_palette(palette),
            term_theme: palette.to_terminal_theme(),
            font_size,
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            pty_events: rx,
            pty_sender: tx,
            show_settings: false,
            show_close_confirm: false,
            notice: None,
            fatal: None,
            fullscreen: false,
            closing: false,
            zoom_accumulator: 0.0,
        };

        app.apply_theme(&cc.egui_ctx);
        if let Some(path) = &app.config.font_path {
            if let Err(err) = install_font(&cc.egui_ctx, path) {
                app.warn(format!("font `{path}` could not be loaded: {err}"));
            }
        }
        if !warnings.is_empty() {
            app.warn(format!(
                "{} problem(s) in the config file: {}",
                warnings.len(),
                warnings.join("; ")
            ));
        } else {
            app.write_starter_config();
        }

        let profile = app.config.default_profile();
        app.spawn_tab(&cc.egui_ctx, &profile);
        app
    }

    /// On the very first run, drop a documented config file next to the
    /// settings panel's "config path" so the options are discoverable.
    fn write_starter_config(&mut self) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        if path.exists() {
            return;
        }
        match self.config.save(&path) {
            Ok(()) => self.note(format!("wrote a starter config to {}", path.display())),
            Err(err) => self.warn(format!("could not write {}: {err}", path.display())),
        }
    }

    pub(crate) fn apply_theme(&mut self, ctx: &Context) {
        self.chrome = Chrome::from_palette(self.palette);
        self.term_theme = self.palette.to_terminal_theme();
        ctx.set_visuals(self.chrome.visuals(self.palette.is_dark()));
    }

    pub(crate) fn set_palette(&mut self, ctx: &Context, palette: &'static Palette) {
        self.palette = palette;
        self.config.theme = palette.name.to_string();
        self.apply_theme(ctx);
    }

    pub(crate) fn note(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            is_error: false,
            shown_at: Instant::now(),
        });
    }

    pub(crate) fn warn(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            is_error: true,
            shown_at: Instant::now(),
        });
    }

    pub(crate) fn spawn_tab(&mut self, ctx: &Context, profile: &Profile) {
        let id = self.next_id;
        let settings = BackendSettings {
            shell: profile.command.clone(),
            args: profile.args.clone(),
            working_directory: std::env::current_dir().ok(),
        };

        match TerminalBackend::new(id, ctx.clone(), self.pty_sender.clone(), settings) {
            Ok(backend) => {
                self.next_id += 1;
                self.tabs.push(Tab {
                    id,
                    backend,
                    title: None,
                    label: profile.name.clone(),
                    unread: false,
                    bell: false,
                    exited: None,
                });
                self.active = self.tabs.len() - 1;
                self.fatal = None;
                self.sync_title(ctx);
            }
            Err(err) => {
                let message = format!("could not start `{}`: {err}", profile.command);
                if self.tabs.is_empty() {
                    self.fatal = Some(message);
                } else {
                    self.warn(message);
                }
            }
        }
    }

    pub(crate) fn close_tab(&mut self, ctx: &Context, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = active_after_close(self.tabs.len(), index, self.active);
        self.tabs.remove(index);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.unread = false;
            tab.bell = false;
        }
        self.sync_title(ctx);
    }

    pub(crate) fn activate(&mut self, ctx: &Context, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        let tab = &mut self.tabs[index];
        tab.unread = false;
        tab.bell = false;
        self.sync_title(ctx);
    }

    fn move_tab(&mut self, delta: i32) {
        if self.tabs.len() < 2 {
            return;
        }
        let target = moved_index(self.tabs.len(), self.active, delta);
        self.tabs.swap(self.active, target);
        self.active = target;
    }

    /// Keep the window title in step with the active tab.
    pub(crate) fn sync_title(&self, ctx: &Context) {
        let title = match self.tabs.get(self.active) {
            Some(tab) if self.tabs.len() > 1 => format!(
                "{} — myterm [{}/{}]",
                tab.display_title(),
                self.active + 1,
                self.tabs.len()
            ),
            Some(tab) => format!("{} — myterm", tab.display_title()),
            None => "myterm".to_string(),
        };
        ctx.send_viewport_cmd(ViewportCommand::Title(title));
    }

    pub(crate) fn terminal_font(&self) -> TerminalFont {
        TerminalFont::new(FontSettings {
            font_type: FontId::new(self.font_size, FontFamily::Monospace),
        })
    }

    fn active_mode(&self) -> TerminalMode {
        self.tabs
            .get(self.active)
            .map(|tab| tab.backend.last_content().terminal_mode)
            .unwrap_or_else(TerminalMode::empty)
    }

    /// Visible rows of the active terminal, used for page-wise scrolling.
    fn active_screen_lines(&self) -> i32 {
        self.tabs
            .get(self.active)
            .map(|tab| tab.backend.last_content().grid.screen_lines() as i32)
            .unwrap_or(1)
    }

    pub(crate) fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(config::MIN_FONT_SIZE, config::MAX_FONT_SIZE);
        self.config.font_size = self.font_size;
    }

    fn scroll_active(&mut self, lines: i32) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.backend.process_command(BackendCommand::Scroll(lines));
        }
    }

    fn write_active(&mut self, bytes: Vec<u8>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if tab.exited.is_none() {
                tab.backend.process_command(BackendCommand::Write(bytes));
            }
        }
    }

    /// Drain everything the PTY threads have queued since the last frame.
    fn handle_pty_events(&mut self, ctx: &Context) {
        let mut events = Vec::new();
        while let Ok(event) = self.pty_events.try_recv() {
            events.push(event);
        }

        for (id, event) in events {
            let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
                continue;
            };

            match event {
                PtyEvent::Wakeup => {
                    self.tabs[index].backend.sync();
                    if index != self.active {
                        self.tabs[index].unread = true;
                    }
                }
                PtyEvent::Title(title) => {
                    self.tabs[index].title = Some(title);
                    if index == self.active {
                        self.sync_title(ctx);
                    }
                }
                PtyEvent::ResetTitle => {
                    self.tabs[index].title = None;
                    if index == self.active {
                        self.sync_title(ctx);
                    }
                }
                // Replies the emulator owes the shell (cursor position reports,
                // device attributes, ...). Without this, programs that query
                // the terminal simply hang.
                PtyEvent::PtyWrite(text) => {
                    self.tabs[index]
                        .backend
                        .process_command(BackendCommand::Write(text.into_bytes()));
                }
                PtyEvent::ColorRequest(color_index, format) => {
                    let color = self.palette.indexed_color(color_index);
                    let reply = format(Rgb {
                        r: color.r(),
                        g: color.g(),
                        b: color.b(),
                    });
                    self.tabs[index]
                        .backend
                        .process_command(BackendCommand::Write(reply.into_bytes()));
                }
                PtyEvent::TextAreaSizeRequest(format) => {
                    let size =
                        WindowSize::from(self.tabs[index].backend.last_content().terminal_size);
                    let reply = format(size);
                    self.tabs[index]
                        .backend
                        .process_command(BackendCommand::Write(reply.into_bytes()));
                }
                PtyEvent::ClipboardStore(_, text) => ctx.copy_text(text),
                PtyEvent::Bell => {
                    if index != self.active {
                        self.tabs[index].bell = true;
                    }
                }
                PtyEvent::ChildExit(code) => {
                    if code == 0 {
                        self.close_tab(ctx, index);
                    } else {
                        // Keep the tab so the exit status stays readable.
                        self.tabs[index].exited = Some(code);
                    }
                }
                PtyEvent::Exit => {
                    if self.tabs[index].exited.is_none() {
                        self.close_tab(ctx, index);
                    }
                }
                _ => {}
            }
        }
    }

    /// Take the keyboard (and ctrl+wheel) events for ourselves before
    /// `TerminalView` sees them, then act on them.
    fn handle_input(&mut self, ctx: &Context) {
        let mode = self.active_mode();
        let (events, mods, zoom) = ctx.input_mut(|input| {
            let mods = input.modifiers;
            let mut taken = Vec::new();
            let mut zoom = 0.0_f32;
            input.events.retain(|event| match event {
                egui::Event::Key { .. }
                | egui::Event::Text(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Paste(_) => {
                    taken.push(event.clone());
                    false
                }
                egui::Event::MouseWheel { unit, delta, .. } if mods.command || mods.ctrl => {
                    zoom += match unit {
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Point => delta.y / 50.0,
                        egui::MouseWheelUnit::Page => delta.y * 3.0,
                    };
                    false
                }
                _ => true,
            });
            (taken, mods, zoom)
        });

        self.zoom_accumulator += zoom;
        while self.zoom_accumulator.abs() >= 1.0 {
            let step = self.zoom_accumulator.signum();
            self.set_font_size(self.font_size + step);
            self.zoom_accumulator -= step;
        }

        for event in events {
            // While a modal is up, the keyboard drives the modal only.
            if self.show_close_confirm {
                if let egui::Event::Key {
                    key, pressed: true, ..
                } = &event
                {
                    match key {
                        egui::Key::Escape => self.show_close_confirm = false,
                        egui::Key::Enter => self.confirm_close(ctx),
                        _ => {}
                    }
                }
                continue;
            }

            // Escape closes the settings panel before anything else sees it.
            if self.show_settings {
                if let egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } = &event
                {
                    self.show_settings = false;
                    continue;
                }
            }

            // A tab whose shell died is a tombstone: Enter, Escape or Space
            // dismisses it, shortcuts keep working, and nothing is typed into
            // the dead PTY.
            if self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.exited.is_some())
            {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = &event
                {
                    if modifiers.is_none()
                        && matches!(key, egui::Key::Enter | egui::Key::Escape | egui::Key::Space)
                    {
                        let active = self.active;
                        self.close_tab(ctx, active);
                        continue;
                    }
                }
                match input::translate(&event, mods, mode) {
                    Some(Action::Write(_)) | Some(Action::Paste(_)) | None => {}
                    Some(action) => self.apply(ctx, action),
                }
                continue;
            }

            if let Some(action) = input::translate(&event, mods, mode) {
                self.apply(ctx, action);
            }
        }
    }

    fn apply(&mut self, ctx: &Context, action: Action) {
        match action {
            Action::Write(bytes) => self.write_active(bytes),
            Action::Copy => {
                if let Some(tab) = self.tabs.get(self.active) {
                    let content = tab.backend.last_content();
                    let selection = selection_text(&content.grid, content.selectable_range);
                    if !selection.is_empty() {
                        ctx.copy_text(selection);
                    }
                }
            }
            Action::Paste(text) => self.write_active(text.into_bytes()),
            Action::NewTab => {
                let profile = self.config.default_profile();
                self.spawn_tab(ctx, &profile);
            }
            Action::CloseTab => {
                let active = self.active;
                self.close_tab(ctx, active);
            }
            Action::NextTab => {
                let next = moved_index(self.tabs.len(), self.active, 1);
                self.activate(ctx, next);
            }
            Action::PrevTab => {
                let prev = moved_index(self.tabs.len(), self.active, -1);
                self.activate(ctx, prev);
            }
            Action::SelectTab(index) => {
                if index < self.tabs.len() {
                    self.activate(ctx, index);
                }
            }
            Action::MoveTab(delta) => self.move_tab(delta),
            Action::NewWindow => self.new_window(),
            Action::Quit => {
                self.closing = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Action::FontDelta(delta) => self.set_font_size(self.font_size + delta),
            Action::FontReset => self.set_font_size(config::DEFAULT_FONT_SIZE),
            Action::ScrollPage(pages) => {
                let lines = pages * self.active_screen_lines().max(1);
                self.scroll_active(lines);
            }
            Action::ScrollToTop => {
                let history = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.backend.last_content().grid.history_size() as i32)
                    .unwrap_or(0);
                self.scroll_active(history);
            }
            Action::ScrollToBottom => {
                let offset = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.backend.last_content().grid.display_offset() as i32)
                    .unwrap_or(0);
                self.scroll_active(-offset);
            }
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
            }
            Action::ToggleSettings => self.show_settings = !self.show_settings,
        }
    }

    /// Launch another copy of myterm.
    fn new_window(&mut self) {
        match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
            Ok(_) => {}
            Err(err) => self.warn(format!("could not open a new window: {err}")),
        }
    }

    pub(crate) fn confirm_close(&mut self, ctx: &Context) {
        self.show_close_confirm = false;
        self.closing = true;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Remember the window geometry so the next run opens the same way.
    fn track_window_geometry(&mut self, ctx: &Context) {
        let (size, maximized) = ctx.input(|i| {
            let info = i.viewport();
            (
                info.inner_rect.map(|rect| rect.size()),
                info.maximized.unwrap_or(false),
            )
        });
        self.config.window_maximized = maximized;
        if let Some(Vec2 { x, y }) = size {
            if !maximized && x >= 200.0 && y >= 150.0 {
                self.config.window_width = x.round();
                self.config.window_height = y.round();
            }
        }
    }

    /// Write the config file, but only when something actually changed.
    pub(crate) fn save_config(&mut self) -> bool {
        if self.config == self.saved_config {
            return true;
        }
        let Some(path) = self.config_path.clone() else {
            return false;
        };
        match self.config.save(&path) {
            Ok(()) => {
                self.saved_config = self.config.clone();
                true
            }
            Err(err) => {
                self.warn(format!("could not write {}: {err}", path.display()));
                false
            }
        }
    }

    /// Re-read the config file, applying theme, font and shell changes.
    pub(crate) fn reload_config(&mut self, ctx: &Context) {
        let (mut config, path) = Config::load();
        // Window geometry is owned by the running window, not the file.
        config.window_width = self.config.window_width;
        config.window_height = self.config.window_height;
        config.window_maximized = self.config.window_maximized;

        let warnings = config.warnings.clone();
        let font_path = config.font_path.clone();
        self.font_size = config.font_size;
        self.config_path = path;
        self.palette = theme::palette_by_name(&config.theme).unwrap_or_else(theme::default_palette);
        self.config = config;
        self.saved_config = self.config.clone();
        self.apply_theme(ctx);

        if let Some(path) = font_path {
            if let Err(err) = install_font(ctx, &path) {
                self.warn(format!("font `{path}` could not be loaded: {err}"));
                return;
            }
        }
        if warnings.is_empty() {
            self.note("configuration reloaded");
        } else {
            self.warn(format!("reloaded with warnings: {}", warnings.join("; ")));
        }
    }
}

/// Use a font file for the terminal grid, ahead of the built-in monospace font.
fn install_font(ctx: &Context, path: &str) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    ctx.add_font(egui::epaint::text::FontInsert::new(
        "myterm-custom",
        egui::FontData::from_owned(bytes),
        vec![egui::epaint::text::InsertFontFamily {
            family: FontFamily::Monospace,
            priority: egui::epaint::text::FontPriority::Highest,
        }],
    ));
    Ok(())
}

impl eframe::App for MyTermApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_pty_events(ctx);
        self.track_window_geometry(ctx);

        if ctx.input(|i| i.viewport().close_requested()) && !self.closing {
            if self.config.confirm_close && self.tabs.len() > 1 {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.show_close_confirm = true;
            } else {
                self.closing = true;
            }
        }

        if self.tabs.is_empty() && self.fatal.is_none() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        self.handle_input(ctx);
        self.draw(ctx);

        if let Some(notice) = &self.notice {
            let age = notice.shown_at.elapsed();
            if age >= NOTICE_TIMEOUT {
                self.notice = None;
            } else {
                ctx.request_repaint_after(NOTICE_TIMEOUT - age);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_selection() {
        // Three tabs, the third is active; closing the first must keep the
        // same tab selected rather than sliding onto its neighbour.
        assert_eq!(active_after_close(3, 0, 2), 1);
        assert_eq!(active_after_close(3, 1, 2), 1);
    }

    #[test]
    fn closing_the_active_tab_selects_a_neighbour() {
        assert_eq!(active_after_close(3, 2, 2), 1);
        assert_eq!(active_after_close(3, 0, 0), 0);
        assert_eq!(active_after_close(2, 1, 1), 0);
    }

    #[test]
    fn closing_a_tab_after_the_active_one_changes_nothing() {
        assert_eq!(active_after_close(3, 2, 0), 0);
        assert_eq!(active_after_close(3, 1, 0), 0);
    }

    #[test]
    fn closing_the_last_tab_is_index_zero() {
        assert_eq!(active_after_close(1, 0, 0), 0);
        assert_eq!(active_after_close(0, 0, 0), 0);
    }

    fn grid_with(rows: &[&str]) -> Grid<Cell> {
        let columns = rows.iter().map(|r| r.len()).max().unwrap_or(1);
        let mut grid: Grid<Cell> = Grid::new(rows.len(), columns, 0);
        for (line, row) in rows.iter().enumerate() {
            for (column, c) in row.chars().enumerate() {
                grid[Point::new(Line(line as i32), Column(column))].c = c;
            }
        }
        grid
    }

    fn whole_grid(grid: &Grid<Cell>) -> Option<SelectionRange> {
        Some(SelectionRange::new(
            Point::new(Line(0), Column(0)),
            Point::new(
                Line(grid.screen_lines() as i32 - 1),
                Column(grid.columns() - 1),
            ),
            false,
        ))
    }

    #[test]
    fn copying_drops_the_padding_at_the_end_of_a_row() {
        let grid = grid_with(&["hello     "]);
        assert_eq!(selection_text(&grid, whole_grid(&grid)), "hello");
    }

    #[test]
    fn copying_keeps_one_line_per_row() {
        let grid = grid_with(&["one  ", "two  ", "three"]);
        assert_eq!(selection_text(&grid, whole_grid(&grid)), "one\ntwo\nthree");
    }

    #[test]
    fn copying_rejoins_a_wrapped_row() {
        let mut grid = grid_with(&["abcde", "fg   "]);
        grid[Point::new(Line(0), Column(4))]
            .flags
            .insert(Flags::WRAPLINE);
        assert_eq!(selection_text(&grid, whole_grid(&grid)), "abcdefg");
    }

    #[test]
    fn copying_without_a_selection_yields_nothing() {
        let grid = grid_with(&["abc"]);
        assert_eq!(selection_text(&grid, None), "");
    }

    #[test]
    fn copying_keeps_blanks_inside_a_line() {
        let grid = grid_with(&["a  b   "]);
        assert_eq!(selection_text(&grid, whole_grid(&grid)), "a  b");
    }

    #[test]
    fn tab_movement_wraps_around() {
        assert_eq!(moved_index(3, 0, 1), 1);
        assert_eq!(moved_index(3, 2, 1), 0);
        assert_eq!(moved_index(3, 0, -1), 2);
        assert_eq!(moved_index(3, 1, -1), 0);
        assert_eq!(moved_index(1, 0, 1), 0);
        assert_eq!(moved_index(0, 0, 1), 0);
    }
}
