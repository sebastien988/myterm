//! Keyboard handling.
//!
//! myterm translates key events itself instead of leaving it to
//! `egui_term::TerminalView`, for two reasons:
//!
//! * the view only forwards keys while the mouse pointer happens to hover the
//!   grid, so typing was lost whenever the pointer sat over the tab bar or
//!   outside the window;
//! * application shortcuts (new tab, zoom, ...) would otherwise also be sent
//!   to the shell as escape sequences.
//!
//! The escape sequences below follow the xterm conventions used by
//! `alacritty_terminal`, including the `CSI 1 ; <modifier> <final>` form for
//! modified cursor keys.

use egui::{Event, Key, Modifiers};
use egui_term::TerminalMode;

/// What a key press should do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Bytes destined for the shell.
    Write(Vec<u8>),
    /// Copy the current selection to the clipboard.
    Copy,
    /// Paste text into the shell (bracketed when the app asked for it).
    Paste(String),
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    SelectTab(usize),
    MoveTab(i32),
    NewWindow,
    Quit,
    /// Change the font size by this many points.
    FontDelta(f32),
    FontReset,
    /// Scroll the viewport by whole screens.
    ScrollPage(i32),
    ScrollToTop,
    ScrollToBottom,
    ToggleFullscreen,
    ToggleSettings,
}

/// Shortcuts as shown in the settings panel and the README.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+Shift+T", "New tab"),
    ("Ctrl+Shift+W", "Close tab"),
    ("Ctrl+Shift+N", "New window"),
    ("Ctrl+Shift+Q", "Quit"),
    ("Ctrl+Tab / Ctrl+Shift+Tab", "Next / previous tab"),
    ("Alt+1 … Alt+9", "Jump to tab"),
    ("Ctrl+Shift+PageUp / PageDown", "Move tab left / right"),
    ("Ctrl+Shift+C / Ctrl+Shift+V", "Copy / paste"),
    ("Shift+Insert", "Paste"),
    ("Shift+PageUp / PageDown", "Scroll one screen"),
    ("Ctrl+Shift+Home / End", "Scroll to top / bottom"),
    ("Ctrl+= / Ctrl+- / Ctrl+0", "Zoom in / out / reset"),
    ("Ctrl+Scroll", "Zoom in / out"),
    ("Alt+Enter", "Toggle fullscreen"),
    ("Ctrl+,", "Settings"),
];

/// Translate one egui event into an [`Action`], or `None` when it carries
/// nothing for the terminal.
///
/// `mods` is the modifier state for the current frame; egui's clipboard events
/// do not carry their own.
pub fn translate(event: &Event, mods: Modifiers, mode: TerminalMode) -> Option<Action> {
    match event {
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => shortcut(*key, *modifiers, mode)
            .or_else(|| encode_key(*key, *modifiers, mode).map(Action::Write)),

        // Printable text. Anything with alt is handled through the key event
        // above (as ESC + char), and egui does not emit text while ctrl is held.
        Event::Text(text) if !mods.alt && !mods.ctrl && !mods.mac_cmd => {
            Some(Action::Write(text.as_bytes().to_vec()))
        }

        // egui turns Ctrl+C / Ctrl+X / Ctrl+V into clipboard events before we
        // ever see the key, so the terminal's control codes are restored here.
        Event::Copy => Some(if mods.shift {
            Action::Copy
        } else {
            Action::Write(vec![0x03])
        }),
        Event::Cut if mods.command || mods.ctrl => Some(Action::Write(vec![0x18])),
        // Windows reports Shift+Delete as a cut; the shell still wants the
        // delete sequence.
        Event::Cut if mods.shift => Some(Action::Write(csi_tilde(3, mods))),
        Event::Paste(text) => Some(if mods.shift {
            Action::Paste(paste_payload(text, mode))
        } else {
            Action::Write(vec![0x16])
        }),
        _ => None,
    }
}

/// Prepare clipboard text for the shell: normalize line endings and, when the
/// application enabled bracketed paste, wrap it so multi-line pastes are not
/// executed line by line.
pub fn paste_payload(text: &str, mode: TerminalMode) -> String {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode.contains(TerminalMode::BRACKETED_PASTE) {
        format!("\x1b[200~{normalized}\x1b[201~")
    } else {
        normalized
    }
}

fn shortcut(key: Key, mods: Modifiers, mode: TerminalMode) -> Option<Action> {
    let ctrl = mods.command || mods.ctrl;
    let alt_only = mods.alt && !ctrl && !mods.shift;
    let ctrl_only = ctrl && !mods.alt && !mods.shift;
    let ctrl_shift = ctrl && mods.shift && !mods.alt;
    // The alternate screen (vim, less, ...) scrolls itself, so scrollback
    // shortcuts stay out of its way.
    let scrollback = !mode.contains(TerminalMode::ALT_SCREEN);

    let action = match key {
        Key::T if ctrl_shift => Action::NewTab,
        Key::W if ctrl_shift => Action::CloseTab,
        Key::N if ctrl_shift => Action::NewWindow,
        Key::Q if ctrl_shift => Action::Quit,
        Key::Tab if ctrl && !mods.shift => Action::NextTab,
        Key::Tab if ctrl_shift => Action::PrevTab,
        Key::PageUp if ctrl_shift => Action::MoveTab(-1),
        Key::PageDown if ctrl_shift => Action::MoveTab(1),
        Key::PageUp if mods.shift && !ctrl && !mods.alt && scrollback => Action::ScrollPage(1),
        Key::PageDown if mods.shift && !ctrl && !mods.alt && scrollback => Action::ScrollPage(-1),
        Key::Home if ctrl_shift && scrollback => Action::ScrollToTop,
        Key::End if ctrl_shift && scrollback => Action::ScrollToBottom,
        Key::Equals | Key::Plus if ctrl_only => Action::FontDelta(1.0),
        Key::Minus if ctrl_only => Action::FontDelta(-1.0),
        Key::Num0 if ctrl_only => Action::FontReset,
        Key::Comma if ctrl_only => Action::ToggleSettings,
        Key::Enter if alt_only => Action::ToggleFullscreen,
        Key::Num1
        | Key::Num2
        | Key::Num3
        | Key::Num4
        | Key::Num5
        | Key::Num6
        | Key::Num7
        | Key::Num8
        | Key::Num9
            if alt_only =>
        {
            Action::SelectTab(digit(key)? as usize - 1)
        }
        _ => return None,
    };
    Some(action)
}

fn digit(key: Key) -> Option<u8> {
    Some(match key {
        Key::Num0 => 0,
        Key::Num1 => 1,
        Key::Num2 => 2,
        Key::Num3 => 3,
        Key::Num4 => 4,
        Key::Num5 => 5,
        Key::Num6 => 6,
        Key::Num7 => 7,
        Key::Num8 => 8,
        Key::Num9 => 9,
        _ => return None,
    })
}

/// The xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
fn modifier_param(mods: Modifiers) -> u8 {
    1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl || mods.command)
}

/// `CSI <number> ~`, with the modifier parameter when any modifier is held.
fn csi_tilde(number: u8, mods: Modifiers) -> Vec<u8> {
    let param = modifier_param(mods);
    if param == 1 {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{param}~").into_bytes()
    }
}

/// Cursor and function keys: `CSI <final>`, `SS3 <final>` in application mode,
/// or `CSI 1 ; <modifier> <final>` when modified.
fn csi_letter(final_byte: char, mods: Modifiers, application: bool) -> Vec<u8> {
    let param = modifier_param(mods);
    if param != 1 {
        format!("\x1b[1;{param}{final_byte}").into_bytes()
    } else if application {
        format!("\x1bO{final_byte}").into_bytes()
    } else {
        format!("\x1b[{final_byte}").into_bytes()
    }
}

/// The character a key produces on its own, used to build ESC-prefixed
/// sequences for alt combinations.
fn key_char(key: Key, shift: bool) -> Option<char> {
    if (Key::A as u8..=Key::Z as u8).contains(&(key as u8)) {
        let offset = key as u8 - Key::A as u8;
        let base = if shift { b'A' } else { b'a' };
        return Some((base + offset) as char);
    }
    if let Some(d) = digit(key) {
        return Some((b'0' + d) as char);
    }
    Some(match key {
        Key::Space => ' ',
        Key::Comma => ',',
        Key::Period => '.',
        Key::Semicolon => ';',
        Key::Colon => ':',
        Key::Quote => '\'',
        Key::Backtick => '`',
        Key::Slash => '/',
        Key::Backslash => '\\',
        Key::Pipe => '|',
        Key::Questionmark => '?',
        Key::Exclamationmark => '!',
        Key::Minus => '-',
        Key::Plus => '+',
        Key::Equals => '=',
        Key::OpenBracket => '[',
        Key::CloseBracket => ']',
        Key::OpenCurlyBracket => '{',
        Key::CloseCurlyBracket => '}',
        _ => return None,
    })
}

/// Encode a key press as the bytes a shell expects, or `None` when the key
/// produces text (handled by [`Event::Text`]) or nothing at all.
pub fn encode_key(key: Key, mods: Modifiers, mode: TerminalMode) -> Option<Vec<u8>> {
    let ctrl = mods.ctrl || mods.command;
    let alt = mods.alt;
    let app_cursor = mode.contains(TerminalMode::APP_CURSOR);

    let bytes = match key {
        Key::Enter => {
            if alt {
                vec![0x1b, b'\r']
            } else {
                vec![b'\r']
            }
        }
        Key::Escape => {
            if alt {
                vec![0x1b, 0x1b]
            } else {
                vec![0x1b]
            }
        }
        Key::Backspace => match (ctrl, alt) {
            (true, true) => vec![0x1b, 0x08],
            (true, false) => vec![0x08],
            (false, true) => vec![0x1b, 0x7f],
            (false, false) => vec![0x7f],
        },
        Key::Tab => {
            if mods.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Space if ctrl => vec![0x00],

        Key::ArrowUp => csi_letter('A', mods, app_cursor),
        Key::ArrowDown => csi_letter('B', mods, app_cursor),
        Key::ArrowRight => csi_letter('C', mods, app_cursor),
        Key::ArrowLeft => csi_letter('D', mods, app_cursor),
        Key::Home => csi_letter('H', mods, app_cursor),
        Key::End => csi_letter('F', mods, app_cursor),

        Key::Insert => {
            // Shift+Insert is paste (delivered as `Event::Paste` by the
            // toolkit on most platforms). Do not also emit CSI Insert, or a
            // paste would be preceded by a junk sequence.
            if mods.shift && !ctrl && !alt {
                return None;
            }
            csi_tilde(2, mods)
        }
        Key::Delete => csi_tilde(3, mods),
        Key::PageUp => csi_tilde(5, mods),
        Key::PageDown => csi_tilde(6, mods),

        Key::F1 => csi_letter('P', mods, true),
        Key::F2 => csi_letter('Q', mods, true),
        Key::F3 => csi_letter('R', mods, true),
        Key::F4 => csi_letter('S', mods, true),
        Key::F5 => csi_tilde(15, mods),
        Key::F6 => csi_tilde(17, mods),
        Key::F7 => csi_tilde(18, mods),
        Key::F8 => csi_tilde(19, mods),
        Key::F9 => csi_tilde(20, mods),
        Key::F10 => csi_tilde(21, mods),
        Key::F11 => csi_tilde(23, mods),
        Key::F12 => csi_tilde(24, mods),

        // Ctrl+letter -> the matching C0 control code.
        Key::A
        | Key::B
        | Key::C
        | Key::D
        | Key::E
        | Key::F
        | Key::G
        | Key::H
        | Key::I
        | Key::J
        | Key::K
        | Key::L
        | Key::M
        | Key::N
        | Key::O
        | Key::P
        | Key::Q
        | Key::R
        | Key::S
        | Key::T
        | Key::U
        | Key::V
        | Key::W
        | Key::X
        | Key::Y
        | Key::Z
            if ctrl =>
        {
            let code = 1 + (key as u8 - Key::A as u8);
            if alt {
                vec![0x1b, code]
            } else {
                vec![code]
            }
        }

        // The remaining control codes that have no letter of their own.
        Key::OpenBracket if ctrl => vec![0x1b],
        Key::Backslash if ctrl => vec![0x1c],
        Key::CloseBracket if ctrl => vec![0x1d],
        Key::Num6 if ctrl => vec![0x1e],
        Key::Num2 if ctrl => vec![0x00],
        Key::Slash | Key::Questionmark | Key::Minus if ctrl => vec![0x1f],

        // Alt+<printable> is the classic "meta" prefix: ESC then the character.
        key if alt && !ctrl => {
            let c = key_char(key, mods.shift)?;
            let mut buf = [0u8; 4];
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            bytes
        }

        _ => return None,
    };

    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, shift: bool, alt: bool) -> Modifiers {
        Modifiers {
            alt,
            ctrl,
            shift,
            mac_cmd: false,
            command: ctrl,
        }
    }

    const NONE: Modifiers = Modifiers::NONE;

    fn encode(key: Key, m: Modifiers, mode: TerminalMode) -> String {
        String::from_utf8(encode_key(key, m, mode).expect("encoded")).unwrap()
    }

    #[test]
    fn plain_keys_use_the_normal_sequences() {
        let mode = TerminalMode::empty();
        assert_eq!(encode(Key::Enter, NONE, mode), "\r");
        assert_eq!(encode(Key::Escape, NONE, mode), "\x1b");
        assert_eq!(encode(Key::Tab, NONE, mode), "\t");
        assert_eq!(encode(Key::Backspace, NONE, mode), "\x7f");
        assert_eq!(encode(Key::ArrowUp, NONE, mode), "\x1b[A");
        assert_eq!(encode(Key::Home, NONE, mode), "\x1b[H");
        assert_eq!(encode(Key::Delete, NONE, mode), "\x1b[3~");
        assert_eq!(encode(Key::F1, NONE, mode), "\x1bOP");
        assert_eq!(encode(Key::F5, NONE, mode), "\x1b[15~");
        assert_eq!(encode(Key::F12, NONE, mode), "\x1b[24~");
    }

    #[test]
    fn application_cursor_mode_switches_arrow_keys() {
        let app = TerminalMode::APP_CURSOR;
        assert_eq!(encode(Key::ArrowUp, NONE, app), "\x1bOA");
        assert_eq!(encode(Key::End, NONE, app), "\x1bOF");
        // Modified arrows keep the CSI form even in application mode.
        assert_eq!(
            encode(Key::ArrowUp, mods(true, false, false), app),
            "\x1b[1;5A"
        );
    }

    #[test]
    fn modifiers_use_the_xterm_parameter() {
        let mode = TerminalMode::empty();
        assert_eq!(
            encode(Key::ArrowRight, mods(false, true, false), mode),
            "\x1b[1;2C"
        );
        assert_eq!(
            encode(Key::ArrowLeft, mods(false, false, true), mode),
            "\x1b[1;3D"
        );
        assert_eq!(
            encode(Key::ArrowDown, mods(true, false, false), mode),
            "\x1b[1;5B"
        );
        assert_eq!(
            encode(Key::ArrowUp, mods(true, true, true), mode),
            "\x1b[1;8A"
        );
        assert_eq!(
            encode(Key::Delete, mods(true, false, false), mode),
            "\x1b[3;5~"
        );
        assert_eq!(
            encode(Key::F5, mods(false, true, false), mode),
            "\x1b[15;2~"
        );
    }

    #[test]
    fn control_codes_cover_the_c0_range() {
        let mode = TerminalMode::empty();
        let ctrl = mods(true, false, false);
        assert_eq!(encode_key(Key::A, ctrl, mode), Some(vec![0x01]));
        assert_eq!(encode_key(Key::C, ctrl, mode), Some(vec![0x03]));
        // Ctrl+U kills the line in every shell; upstream egui_term sends 'Q' here.
        assert_eq!(encode_key(Key::U, ctrl, mode), Some(vec![0x15]));
        assert_eq!(encode_key(Key::Z, ctrl, mode), Some(vec![0x1a]));
        assert_eq!(encode_key(Key::Space, ctrl, mode), Some(vec![0x00]));
        assert_eq!(encode_key(Key::OpenBracket, ctrl, mode), Some(vec![0x1b]));
        assert_eq!(encode_key(Key::Backslash, ctrl, mode), Some(vec![0x1c]));
        assert_eq!(encode_key(Key::Num6, ctrl, mode), Some(vec![0x1e]));
        assert_eq!(encode_key(Key::Slash, ctrl, mode), Some(vec![0x1f]));
        assert_eq!(
            encode_key(Key::D, mods(true, false, true), mode),
            Some(vec![0x1b, 0x04])
        );
    }

    #[test]
    fn alt_prefixes_printable_keys_with_escape() {
        let mode = TerminalMode::empty();
        let alt = mods(false, false, true);
        assert_eq!(encode(Key::B, alt, mode), "\x1bb");
        assert_eq!(encode(Key::F, mods(false, true, true), mode), "\x1bF");
        assert_eq!(encode(Key::Period, alt, mode), "\x1b.");
        assert_eq!(encode(Key::Backspace, alt, mode), "\x1b\x7f");
        assert_eq!(encode(Key::Enter, alt, mode), "\x1b\r");
    }

    #[test]
    fn shift_tab_sends_backtab() {
        let mode = TerminalMode::empty();
        assert_eq!(encode(Key::Tab, mods(false, true, false), mode), "\x1b[Z");
    }

    #[test]
    fn keys_without_a_sequence_are_left_to_text_events() {
        let mode = TerminalMode::empty();
        assert_eq!(encode_key(Key::A, NONE, mode), None);
        assert_eq!(encode_key(Key::Num1, NONE, mode), None);
        assert_eq!(encode_key(Key::Space, NONE, mode), None);
    }

    fn key_event(key: Key, m: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        }
    }

    #[test]
    fn shortcuts_win_over_terminal_encoding() {
        let mode = TerminalMode::empty();
        let ctrl_shift = mods(true, true, false);
        for (key, expected) in [
            (Key::T, Action::NewTab),
            (Key::W, Action::CloseTab),
            (Key::N, Action::NewWindow),
            (Key::Q, Action::Quit),
            (Key::Tab, Action::PrevTab),
            (Key::PageUp, Action::MoveTab(-1)),
            (Key::PageDown, Action::MoveTab(1)),
            (Key::Home, Action::ScrollToTop),
            (Key::End, Action::ScrollToBottom),
        ] {
            assert_eq!(
                translate(&key_event(key, ctrl_shift), ctrl_shift, mode),
                Some(expected),
                "{key:?}"
            );
        }

        let ctrl = mods(true, false, false);
        assert_eq!(
            translate(&key_event(Key::Tab, ctrl), ctrl, mode),
            Some(Action::NextTab)
        );
        assert_eq!(
            translate(&key_event(Key::Equals, ctrl), ctrl, mode),
            Some(Action::FontDelta(1.0))
        );
        assert_eq!(
            translate(&key_event(Key::Num0, ctrl), ctrl, mode),
            Some(Action::FontReset)
        );
        assert_eq!(
            translate(&key_event(Key::Comma, ctrl), ctrl, mode),
            Some(Action::ToggleSettings)
        );

        let alt = mods(false, false, true);
        assert_eq!(
            translate(&key_event(Key::Num3, alt), alt, mode),
            Some(Action::SelectTab(2))
        );
        assert_eq!(
            translate(&key_event(Key::Enter, alt), alt, mode),
            Some(Action::ToggleFullscreen)
        );
    }

    #[test]
    fn ctrl_shift_letters_still_reach_the_shell() {
        // Only the handful of letters bound above are stolen from the terminal.
        let mode = TerminalMode::empty();
        let m = mods(true, true, false);
        assert_eq!(
            translate(&key_event(Key::L, m), m, mode),
            Some(Action::Write(vec![0x0c]))
        );
    }

    #[test]
    fn scrollback_shortcuts_yield_to_the_alternate_screen() {
        let shift = mods(false, true, false);
        assert_eq!(
            translate(&key_event(Key::PageUp, shift), shift, TerminalMode::empty()),
            Some(Action::ScrollPage(1))
        );
        assert_eq!(
            translate(
                &key_event(Key::PageUp, shift),
                shift,
                TerminalMode::ALT_SCREEN
            ),
            Some(Action::Write(b"\x1b[5;2~".to_vec()))
        );
    }

    #[test]
    fn clipboard_events_keep_terminal_semantics() {
        let mode = TerminalMode::empty();
        let ctrl = mods(true, false, false);
        let ctrl_shift = mods(true, true, false);

        // Ctrl+C interrupts, Ctrl+Shift+C copies.
        assert_eq!(
            translate(&Event::Copy, ctrl, mode),
            Some(Action::Write(vec![0x03]))
        );
        assert_eq!(
            translate(&Event::Copy, ctrl_shift, mode),
            Some(Action::Copy)
        );
        assert_eq!(
            translate(&Event::Cut, ctrl, mode),
            Some(Action::Write(vec![0x18]))
        );
        // Shift+Delete arrives as a cut event on Windows.
        assert_eq!(
            translate(&Event::Cut, mods(false, true, false), mode),
            Some(Action::Write(b"\x1b[3;2~".to_vec()))
        );
        assert_eq!(
            translate(&Event::Paste("hi".into()), ctrl, mode),
            Some(Action::Write(vec![0x16]))
        );
        assert_eq!(
            translate(&Event::Paste("hi".into()), ctrl_shift, mode),
            Some(Action::Paste("hi".into()))
        );
    }

    #[test]
    fn pasting_normalizes_newlines_and_brackets_when_asked() {
        assert_eq!(paste_payload("a\r\nb\nc", TerminalMode::empty()), "a\rb\rc");
        assert_eq!(
            paste_payload("a\nb", TerminalMode::BRACKETED_PASTE),
            "\x1b[200~a\rb\x1b[201~"
        );
    }

    #[test]
    fn text_events_pass_through_unless_a_modifier_claims_them() {
        let mode = TerminalMode::empty();
        assert_eq!(
            translate(&Event::Text("é".into()), NONE, mode),
            Some(Action::Write("é".as_bytes().to_vec()))
        );
        assert_eq!(
            translate(&Event::Text("b".into()), mods(false, false, true), mode),
            None
        );
    }

    #[test]
    fn shift_insert_is_left_to_the_paste_event() {
        let mode = TerminalMode::empty();
        let shift = mods(false, true, false);
        assert_eq!(encode_key(Key::Insert, shift, mode), None);
        assert_eq!(translate(&key_event(Key::Insert, shift), shift, mode), None);
        // Unmodified Insert still sends the usual CSI sequence.
        assert_eq!(encode(Key::Insert, NONE, mode), "\x1b[2~");
        // Ctrl+Insert is a modified insert, not paste.
        assert_eq!(
            encode(Key::Insert, mods(true, false, false), mode),
            "\x1b[2;5~"
        );
    }

    #[test]
    fn key_releases_are_ignored() {
        let event = Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: NONE,
        };
        assert_eq!(translate(&event, NONE, TerminalMode::empty()), None);
    }
}
