//! Color palettes for the terminal grid and for the app's own chrome.
//!
//! A [`Palette`] describes the 16 ANSI colors plus foreground/background. The
//! window chrome (tab bar, panels, popups) is *derived* from that palette by
//! [`Chrome`], so switching themes recolors the whole app instead of leaving a
//! hard-coded dark tab bar on top of a light terminal.

use egui::{Color32, Visuals};
use egui_term::{ColorPalette, TerminalTheme};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// A terminal color scheme: 16 ANSI colors, plus fore/background and an accent
/// used for the active tab and other highlights.
#[derive(Debug, Clone)]
pub struct Palette {
    pub name: &'static str,
    pub background: Color32,
    pub foreground: Color32,
    pub accent: Color32,
    /// Black, red, green, yellow, blue, magenta, cyan, white.
    pub normal: [Color32; 8],
    /// The bright variants, in the same order.
    pub bright: [Color32; 8],
}

pub const PALETTES: &[Palette] = &[
    Palette {
        name: "myterm",
        background: hex(0x121216),
        foreground: hex(0xd7d7e0),
        accent: hex(0xa06cff),
        normal: [
            hex(0x2a2a34),
            hex(0xf07178),
            hex(0xa6cd7a),
            hex(0xffcb6b),
            hex(0x82aaff),
            hex(0xc792ea),
            hex(0x89ddff),
            hex(0xd7d7e0),
        ],
        bright: [
            hex(0x4a4a5a),
            hex(0xff8b92),
            hex(0xbde08f),
            hex(0xffd98a),
            hex(0x9ec1ff),
            hex(0xdaaaf5),
            hex(0xa5e8ff),
            hex(0xf5f5fa),
        ],
    },
    Palette {
        name: "alacritty",
        background: hex(0x181818),
        foreground: hex(0xd8d8d8),
        accent: hex(0xaa759f),
        normal: [
            hex(0x181818),
            hex(0xac4242),
            hex(0x90a959),
            hex(0xf4bf75),
            hex(0x6a9fb5),
            hex(0xaa759f),
            hex(0x75b5aa),
            hex(0xd8d8d8),
        ],
        bright: [
            hex(0x6b6b6b),
            hex(0xc55555),
            hex(0xaac474),
            hex(0xfeca88),
            hex(0x82b8c8),
            hex(0xc28cb8),
            hex(0x93d3c3),
            hex(0xf8f8f8),
        ],
    },
    Palette {
        name: "gruvbox-dark",
        background: hex(0x282828),
        foreground: hex(0xebdbb2),
        accent: hex(0xd3869b),
        normal: [
            hex(0x282828),
            hex(0xcc241d),
            hex(0x98971a),
            hex(0xd79921),
            hex(0x458588),
            hex(0xb16286),
            hex(0x689d6a),
            hex(0xa89984),
        ],
        bright: [
            hex(0x928374),
            hex(0xfb4934),
            hex(0xb8bb26),
            hex(0xfabd2f),
            hex(0x83a598),
            hex(0xd3869b),
            hex(0x8ec07c),
            hex(0xebdbb2),
        ],
    },
    Palette {
        name: "solarized-dark",
        background: hex(0x002b36),
        foreground: hex(0x93a1a1),
        accent: hex(0x2aa198),
        normal: [
            hex(0x073642),
            hex(0xdc322f),
            hex(0x859900),
            hex(0xb58900),
            hex(0x268bd2),
            hex(0xd33682),
            hex(0x2aa198),
            hex(0xeee8d5),
        ],
        bright: [
            hex(0x002b36),
            hex(0xcb4b16),
            hex(0x586e75),
            hex(0x657b83),
            hex(0x839496),
            hex(0x6c71c4),
            hex(0x93a1a1),
            hex(0xfdf6e3),
        ],
    },
    Palette {
        name: "solarized-light",
        background: hex(0xfdf6e3),
        foreground: hex(0x586e75),
        accent: hex(0x268bd2),
        normal: [
            hex(0x073642),
            hex(0xdc322f),
            hex(0x859900),
            hex(0xb58900),
            hex(0x268bd2),
            hex(0xd33682),
            hex(0x2aa198),
            hex(0xeee8d5),
        ],
        bright: [
            hex(0x002b36),
            hex(0xcb4b16),
            hex(0x586e75),
            hex(0x657b83),
            hex(0x839496),
            hex(0x6c71c4),
            hex(0x93a1a1),
            hex(0xfdf6e3),
        ],
    },
];

/// The palette used when the configured name is unknown (or absent).
pub fn default_palette() -> &'static Palette {
    &PALETTES[0]
}

/// Look a palette up by name, case-insensitively.
pub fn palette_by_name(name: &str) -> Option<&'static Palette> {
    PALETTES
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

fn to_hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// Perceived brightness, 0.0 (black) to 1.0 (white).
fn luminance(color: Color32) -> f32 {
    (0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32) / 255.0
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

impl Palette {
    pub fn is_dark(&self) -> bool {
        luminance(self.background) < 0.5
    }

    /// Nudge a color away from the background: lighter on dark themes, darker
    /// on light ones.
    fn lift(&self, color: Color32, amount: f32) -> Color32 {
        let target = if self.is_dark() {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        mix(color, target, amount)
    }

    /// Translate into the palette type `egui_term` wants for the grid.
    ///
    /// Dim colors are not part of a [`Palette`]; they are derived by pulling
    /// each normal color two thirds of the way toward the background.
    pub fn to_term_palette(&self) -> ColorPalette {
        let dim = |c: Color32| to_hex(mix(c, self.background, 0.35));
        ColorPalette {
            foreground: to_hex(self.foreground),
            background: to_hex(self.background),
            black: to_hex(self.normal[0]),
            red: to_hex(self.normal[1]),
            green: to_hex(self.normal[2]),
            yellow: to_hex(self.normal[3]),
            blue: to_hex(self.normal[4]),
            magenta: to_hex(self.normal[5]),
            cyan: to_hex(self.normal[6]),
            white: to_hex(self.normal[7]),
            bright_black: to_hex(self.bright[0]),
            bright_red: to_hex(self.bright[1]),
            bright_green: to_hex(self.bright[2]),
            bright_yellow: to_hex(self.bright[3]),
            bright_blue: to_hex(self.bright[4]),
            bright_magenta: to_hex(self.bright[5]),
            bright_cyan: to_hex(self.bright[6]),
            bright_white: to_hex(self.bright[7]),
            bright_foreground: Some(to_hex(self.lift(self.foreground, 0.25))),
            dim_foreground: dim(self.foreground),
            dim_black: dim(self.normal[0]),
            dim_red: dim(self.normal[1]),
            dim_green: dim(self.normal[2]),
            dim_yellow: dim(self.normal[3]),
            dim_blue: dim(self.normal[4]),
            dim_magenta: dim(self.normal[5]),
            dim_cyan: dim(self.normal[6]),
            dim_white: dim(self.normal[7]),
        }
    }

    pub fn to_terminal_theme(&self) -> TerminalTheme {
        TerminalTheme::new(Box::new(self.to_term_palette()))
    }

    /// The color an `OSC 4 / 10 / 11` query should be answered with.
    ///
    /// Indices follow `alacritty_terminal`: 0-15 are the ANSI colors, 16-255
    /// the 256-color cube, 256 foreground, 257 background, 258 cursor.
    pub fn indexed_color(&self, index: usize) -> Color32 {
        match index {
            0..=7 => self.normal[index],
            8..=15 => self.bright[index - 8],
            16..=231 => {
                let i = index - 16;
                let level = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
                Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            232..=255 => {
                let v = ((index - 232) * 10 + 8) as u8;
                Color32::from_rgb(v, v, v)
            }
            257 => self.background,
            _ => self.foreground,
        }
    }
}

/// Colors for the app's own widgets, derived from the active [`Palette`].
#[derive(Debug, Clone)]
pub struct Chrome {
    pub window_bg: Color32,
    pub tabbar_bg: Color32,
    pub tab_active: Color32,
    pub tab_inactive: Color32,
    pub tab_hover: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub warn: Color32,
}

impl Chrome {
    pub fn from_palette(palette: &Palette) -> Self {
        let bg = palette.background;
        Self {
            window_bg: bg,
            tabbar_bg: palette.lift(bg, 0.05),
            tab_active: palette.lift(bg, 0.19),
            tab_inactive: palette.lift(bg, 0.09),
            tab_hover: palette.lift(bg, 0.14),
            text: palette.lift(palette.foreground, 0.25),
            text_dim: mix(palette.foreground, bg, 0.35),
            accent: palette.accent,
            border: palette.lift(bg, 0.16),
            warn: palette.bright[3],
        }
    }

    /// egui styling for the parts of the UI that are not the terminal grid.
    pub fn visuals(&self, dark: bool) -> Visuals {
        let mut visuals = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.panel_fill = self.window_bg;
        visuals.window_fill = self.tabbar_bg;
        visuals.extreme_bg_color = self.window_bg;
        visuals.faint_bg_color = self.tab_inactive;
        visuals.window_stroke = egui::Stroke::new(1.0, self.border);
        visuals.selection.bg_fill = self.accent.gamma_multiply(0.35);
        visuals.selection.stroke = egui::Stroke::new(1.0, self.accent);
        visuals.hyperlink_color = self.accent;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_dim);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text);
        visuals.widgets.inactive.bg_fill = self.tab_inactive;
        visuals.widgets.inactive.weak_bg_fill = self.tab_inactive;
        visuals.widgets.hovered.bg_fill = self.tab_hover;
        visuals.widgets.hovered.weak_bg_fill = self.tab_hover;
        visuals.widgets.active.bg_fill = self.tab_active;
        visuals.widgets.active.weak_bg_fill = self.tab_active;
        visuals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_renders_valid_hex() {
        // egui_term parses these with a strict "#rrggbb" length check and
        // panics on anything else, so every generated string must be 7 chars.
        for palette in PALETTES {
            let term = palette.to_term_palette();
            for color in [
                term.foreground,
                term.background,
                term.black,
                term.bright_white,
                term.dim_red,
                term.bright_foreground.clone().unwrap_or_default(),
            ] {
                assert_eq!(color.len(), 7, "{}: {color}", palette.name);
                assert!(color.starts_with('#'));
                assert!(u32::from_str_radix(&color[1..], 16).is_ok());
            }
        }
    }

    #[test]
    fn palette_names_are_unique_and_findable() {
        for palette in PALETTES {
            let found = palette_by_name(palette.name).expect("palette by name");
            assert_eq!(found.name, palette.name);
        }
        assert_eq!(palette_by_name("MYTERM").map(|p| p.name), Some("myterm"));
        assert!(palette_by_name("nope").is_none());
    }

    #[test]
    fn light_and_dark_palettes_are_classified() {
        assert!(palette_by_name("myterm").unwrap().is_dark());
        assert!(!palette_by_name("solarized-light").unwrap().is_dark());
    }

    #[test]
    fn indexed_colors_follow_the_xterm_cube() {
        let palette = default_palette();
        assert_eq!(palette.indexed_color(1), palette.normal[1]);
        assert_eq!(palette.indexed_color(9), palette.bright[1]);
        assert_eq!(palette.indexed_color(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(palette.indexed_color(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(palette.indexed_color(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(palette.indexed_color(256), palette.foreground);
        assert_eq!(palette.indexed_color(257), palette.background);
    }

    #[test]
    fn chrome_separates_tab_bar_from_background() {
        for palette in PALETTES {
            let chrome = Chrome::from_palette(palette);
            assert_ne!(chrome.tabbar_bg, chrome.window_bg, "{}", palette.name);
            assert_ne!(chrome.tab_active, chrome.tab_inactive, "{}", palette.name);
        }
    }
}
