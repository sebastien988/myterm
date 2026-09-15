//! App chrome colors, plus the palette handed to the terminal widget.
//!
//! The terminal grid is painted by `egui_term` from its own [`ColorPalette`],
//! not by the surrounding egui visuals, so the two have to be kept in sync
//! here or the terminal area ends up a slightly different shade than the rest
//! of the window.

use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use egui::Color32;
use egui_term::{ColorPalette, TerminalTheme};

pub const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x16);
pub const FG: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xe0);
pub const TABBAR_BG: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x20);
pub const TAB_ACTIVE: Color32 = Color32::from_rgb(0x24, 0x24, 0x2e);
pub const TAB_INACTIVE: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x20);
pub const ACCENT: Color32 = Color32::from_rgb(0xa0, 0x6c, 0xff);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xab);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x7a, 0x7a);
/// The mark on a tab that rang the bell — warmer than the activity accent, so
/// the two read apart at a glance.
pub const BELL: Color32 = Color32::from_rgb(0xf4, 0xbf, 0x75);

/// Formats a color as `#rrggbb`.
///
/// That is the only form `egui_term`'s palette parser accepts — it panics on
/// anything else — so every palette entry we set goes through here.
pub fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// The terminal palette: `egui_term`'s defaults with the background and
/// foreground pulled over to match the app chrome.
pub fn palette() -> ColorPalette {
    ColorPalette {
        background: hex(BG),
        foreground: hex(FG),
        ..ColorPalette::default()
    }
}

pub fn terminal_theme() -> TerminalTheme {
    TerminalTheme::new(Box::new(palette()))
}

/// Resolves an OSC color index (`OSC 4`, `OSC 10`, `OSC 11`) to the color we
/// actually render it with, so color queries can be answered.
///
/// Indices `0..=255` are the ANSI palette; from 256 up they are the
/// [`NamedColor`] discriminants.
pub fn osc_color(theme: &TerminalTheme, index: usize) -> Color32 {
    let color = match index {
        0..=255 => AnsiColor::Indexed(index as u8),
        i if i == NamedColor::Background as usize => AnsiColor::Named(NamedColor::Background),
        // The cursor is drawn in the foreground color, and anything beyond the
        // named range is something we do not track separately. Answering with
        // the foreground beats leaving the query unanswered — a program that
        // asks and gets nothing back waits for its own timeout.
        _ => AnsiColor::Named(NamedColor::Foreground),
    };

    theme.get_color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_uses_the_format_egui_term_can_parse() {
        // `egui_term::theme::hex_to_color` panics unless this is exactly
        // `#rrggbb`, and it is called while painting every cell.
        let formatted = hex(Color32::from_rgb(0x01, 0xab, 0xff));
        assert_eq!(formatted, "#01abff");
        assert_eq!(formatted.len(), 7);
    }

    #[test]
    fn terminal_background_matches_the_app_chrome() {
        let theme = terminal_theme();
        assert_eq!(
            theme.get_color(AnsiColor::Named(NamedColor::Background)),
            BG
        );
        assert_eq!(
            theme.get_color(AnsiColor::Named(NamedColor::Foreground)),
            FG
        );
    }

    #[test]
    fn osc_color_maps_the_whole_index_space() {
        let theme = terminal_theme();

        assert_eq!(osc_color(&theme, NamedColor::Background as usize), BG);
        assert_eq!(osc_color(&theme, NamedColor::Foreground as usize), FG);
        assert_eq!(osc_color(&theme, 1), theme.get_color(AnsiColor::Indexed(1)));

        // Every index has to produce an answer rather than panic.
        for index in 0..300 {
            let _ = osc_color(&theme, index);
        }
    }
}
