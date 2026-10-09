//! Built-in color schemes (à la Windows Terminal / iTerm2 / Alacritty) and the
//! app-chrome colors derived from them.

use egui::Color32;
use egui_term::{ColorPalette, TerminalTheme};

pub struct Scheme {
    pub name: &'static str,
    pub background: &'static str,
    pub foreground: &'static str,
    pub accent: &'static str,
    /// black, red, green, yellow, blue, magenta, cyan, white, then the
    /// bright variants in the same order.
    pub ansi: [&'static str; 16],
}

pub const SCHEMES: &[Scheme] = &[
    Scheme {
        name: "myterm",
        background: "#121216",
        foreground: "#e6e6ef",
        accent: "#a06cff",
        ansi: [
            "#1e1e26", "#ff6b81", "#7ee787", "#f9d77e", "#79a8ff", "#c58cff", "#6fe3e3", "#d8d8e4",
            "#5a5a6e", "#ff8fa0", "#a2f0aa", "#ffe6a3", "#a1c2ff", "#dab3ff", "#9af0f0", "#ffffff",
        ],
    },
    Scheme {
        name: "Dracula",
        background: "#282a36",
        foreground: "#f8f8f2",
        accent: "#bd93f9",
        ansi: [
            "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
            "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
        ],
    },
    Scheme {
        name: "Catppuccin Mocha",
        background: "#1e1e2e",
        foreground: "#cdd6f4",
        accent: "#cba6f7",
        ansi: [
            "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
            "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#a6adc8",
        ],
    },
    Scheme {
        name: "Tokyo Night",
        background: "#1a1b26",
        foreground: "#c0caf5",
        accent: "#7aa2f7",
        ansi: [
            "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
            "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
        ],
    },
    Scheme {
        name: "One Dark",
        background: "#282c34",
        foreground: "#abb2bf",
        accent: "#61afef",
        ansi: [
            "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
            "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
        ],
    },
    Scheme {
        name: "Gruvbox Dark",
        background: "#282828",
        foreground: "#ebdbb2",
        accent: "#fe8019",
        ansi: [
            "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#a89984",
            "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
        ],
    },
    Scheme {
        name: "Nord",
        background: "#2e3440",
        foreground: "#d8dee9",
        accent: "#88c0d0",
        ansi: [
            "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
            "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
        ],
    },
    Scheme {
        name: "Solarized Dark",
        background: "#002b36",
        foreground: "#839496",
        accent: "#268bd2",
        ansi: [
            "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
            "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
        ],
    },
    Scheme {
        name: "Solarized Light",
        background: "#fdf6e3",
        foreground: "#657b83",
        accent: "#268bd2",
        ansi: [
            "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
            "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
        ],
    },
];

pub fn scheme_index(name: &str) -> usize {
    SCHEMES.iter().position(|s| s.name == name).unwrap_or(0)
}

/// A scheme resolved into terminal colors plus matching UI chrome colors.
pub struct Theme {
    pub terminal: TerminalTheme,
    pub dark: bool,
    pub bg: Color32,
    pub fg: Color32,
    pub accent: Color32,
    pub tabbar: Color32,
    pub tab_active: Color32,
    pub text_dim: Color32,
    pub divider: Color32,
    /// Overlay painted over unfocused split panes.
    pub inactive_overlay: Color32,
}

impl Theme {
    pub fn from_scheme(s: &Scheme) -> Self {
        let bg = hex(s.background);
        let fg = hex(s.foreground);
        let dark = luminance(bg) < 0.5;
        let c = |i: usize| s.ansi[i].to_string();
        let dim = |i: usize| to_hex(mix(hex(s.ansi[i]), Color32::BLACK, 0.34));

        let palette = ColorPalette {
            foreground: s.foreground.to_string(),
            background: s.background.to_string(),
            black: c(0),
            red: c(1),
            green: c(2),
            yellow: c(3),
            blue: c(4),
            magenta: c(5),
            cyan: c(6),
            white: c(7),
            bright_black: c(8),
            bright_red: c(9),
            bright_green: c(10),
            bright_yellow: c(11),
            bright_blue: c(12),
            bright_magenta: c(13),
            bright_cyan: c(14),
            bright_white: c(15),
            bright_foreground: None,
            dim_foreground: to_hex(mix(fg, bg, 0.34)),
            dim_black: dim(0),
            dim_red: dim(1),
            dim_green: dim(2),
            dim_yellow: dim(3),
            dim_blue: dim(4),
            dim_magenta: dim(5),
            dim_cyan: dim(6),
            dim_white: dim(7),
        };

        Self {
            terminal: TerminalTheme::new(Box::new(palette)),
            dark,
            bg,
            fg,
            accent: hex(s.accent),
            tabbar: mix(bg, fg, 0.04),
            tab_active: mix(bg, fg, 0.10),
            text_dim: mix(bg, fg, 0.60),
            divider: mix(bg, fg, 0.12),
            inactive_overlay: if dark {
                Color32::from_black_alpha(80)
            } else {
                Color32::from_white_alpha(90)
            },
        }
    }

    pub fn visuals(&self) -> egui::Visuals {
        let mut v = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = self.bg;
        v.window_fill = self.tabbar;
        v.extreme_bg_color = self.bg;
        v.override_text_color = Some(self.fg);
        v.selection.bg_fill = self.accent.gamma_multiply(0.45);
        v.selection.stroke.color = self.accent;
        v.hyperlink_color = self.accent;
        v
    }
}

fn hex(s: &str) -> Color32 {
    let s = s.trim_start_matches('#');
    let channel = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0);
    Color32::from_rgb(channel(0), channel(2), channel(4))
}

fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Linear blend from `a` (t = 0) to `b` (t = 1).
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

fn luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}
