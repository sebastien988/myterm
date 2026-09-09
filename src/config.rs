//! The `myterm.conf` settings file.
//!
//! Deliberately dependency-free: `key = value` lines with `#` comments. The
//! file is only rewritten when settings actually change, and rewriting keeps
//! unknown keys, ordering, and comments intact so a hand-edited file survives.

use std::path::PathBuf;

/// A shell that can be launched from the new-tab menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

pub const MIN_FONT_SIZE: f32 = 6.0;
pub const MAX_FONT_SIZE: f32 = 48.0;
pub const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Shell for new tabs; `None` means "pick the platform default".
    pub shell: Option<String>,
    pub shell_args: Vec<String>,
    pub font_size: f32,
    /// A `.ttf`/`.otf` file used for the terminal grid instead of the built-in font.
    pub font_path: Option<String>,
    pub theme: String,
    /// Hide the tab bar while only one tab is open.
    pub tab_bar_auto_hide: bool,
    /// Ask before closing a window that still has several tabs.
    pub confirm_close: bool,
    pub scrollbar: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub window_maximized: bool,
    /// Extra shells listed in the new-tab menu.
    pub profiles: Vec<Profile>,
    /// Problems found while parsing, surfaced in the UI rather than swallowed.
    pub warnings: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: None,
            shell_args: Vec::new(),
            font_size: DEFAULT_FONT_SIZE,
            font_path: None,
            theme: crate::theme::default_palette().name.to_string(),
            tab_bar_auto_hide: false,
            confirm_close: true,
            scrollbar: true,
            window_width: 1100.0,
            window_height: 700.0,
            window_maximized: false,
            profiles: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Written on first run, and used as the skeleton when saving a config that
/// does not exist on disk yet.
const TEMPLATE: &str = r##"# myterm configuration
#
# Lines are `key = value`; everything after a `#` starts a comment.
# myterm rewrites the values it manages when you save from the settings
# panel, but keeps your comments, ordering and any extra keys.

# Shell launched in new tabs. Leave unset for the platform default
# (powershell.exe on Windows, $SHELL elsewhere).
# shell = powershell.exe
# shell_args = -NoLogo

# Terminal font size in points, and an optional font file to use for the grid.
font_size = 14
# font_path = C:\Windows\Fonts\CascadiaMono.ttf

# Color scheme: myterm, alacritty, gruvbox-dark, solarized-dark, solarized-light
theme = myterm

# Hide the tab bar while a single tab is open.
tab_bar_auto_hide = false

# Ask for confirmation when closing a window with several open tabs.
confirm_close = true

# Show the scrollback position indicator on the right edge.
scrollbar = true

# Window geometry, remembered between runs.
window_width = 1100
window_height = 700
window_maximized = false

# Extra shells for the new-tab menu: `profile.<label> = <command> [args...]`.
# Quote any argument that contains spaces.
# profile.PowerShell = powershell.exe -NoLogo
# profile.Command Prompt = cmd.exe
"##;

/// Keys whose values myterm owns and rewrites on save.
const MANAGED_KEYS: &[&str] = &[
    "shell",
    "shell_args",
    "font_size",
    "font_path",
    "theme",
    "tab_bar_auto_hide",
    "confirm_close",
    "scrollbar",
    "window_width",
    "window_height",
    "window_maximized",
];

/// `%APPDATA%\myterm\myterm.conf` on Windows, `$XDG_CONFIG_HOME/myterm/myterm.conf`
/// (or `~/.config/...`) elsewhere. `MYTERM_CONFIG` overrides both.
pub fn config_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MYTERM_CONFIG") {
        if !explicit.trim().is_empty() {
            return Some(PathBuf::from(explicit));
        }
    }

    let dir = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;

    Some(dir.join("myterm").join("myterm.conf"))
}

/// The shell used when the config does not name one.
pub fn platform_default_shell() -> String {
    if cfg!(windows) {
        std::env::var("MYTERM_SHELL").unwrap_or_else(|_| "powershell.exe".to_string())
    } else {
        std::env::var("MYTERM_SHELL")
            .or_else(|_| std::env::var("SHELL"))
            .unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Split a command line into a program and its arguments, honoring double
/// quotes so Windows paths with spaces survive.
pub fn split_command(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(current);
    }
    out
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Split a line into `(key, value)`, ignoring comments and blank lines.
fn split_entry(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

impl Config {
    /// Parse a config file. Unparsable values keep their default and are
    /// reported through [`Config::warnings`].
    pub fn parse(text: &str) -> Self {
        let mut config = Config::default();
        config.profiles.clear();

        for (number, line) in text.lines().enumerate() {
            let Some((key, value)) = split_entry(line) else {
                continue;
            };
            let line_no = number + 1;
            let mut bad = |what: &str| {
                config
                    .warnings
                    .push(format!("line {line_no}: {what} for `{key}`"));
            };

            match key {
                "shell" => {
                    config.shell = (!value.is_empty()).then(|| value.to_string());
                }
                "shell_args" => config.shell_args = split_command(value),
                "font_size" => match value.parse::<f32>() {
                    Ok(size) if size.is_finite() => {
                        config.font_size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                    }
                    _ => bad("expected a number"),
                },
                "font_path" => {
                    config.font_path = (!value.is_empty()).then(|| value.to_string());
                }
                "theme" => {
                    if crate::theme::palette_by_name(value).is_some() {
                        config.theme = value.to_string();
                    } else {
                        bad("unknown theme");
                    }
                }
                "tab_bar_auto_hide" => match parse_bool(value) {
                    Some(v) => config.tab_bar_auto_hide = v,
                    None => bad("expected true or false"),
                },
                "confirm_close" => match parse_bool(value) {
                    Some(v) => config.confirm_close = v,
                    None => bad("expected true or false"),
                },
                "scrollbar" => match parse_bool(value) {
                    Some(v) => config.scrollbar = v,
                    None => bad("expected true or false"),
                },
                "window_width" => match value.parse::<f32>() {
                    Ok(v) if v >= 200.0 => config.window_width = v,
                    _ => bad("expected a width of at least 200"),
                },
                "window_height" => match value.parse::<f32>() {
                    Ok(v) if v >= 150.0 => config.window_height = v,
                    _ => bad("expected a height of at least 150"),
                },
                "window_maximized" => match parse_bool(value) {
                    Some(v) => config.window_maximized = v,
                    None => bad("expected true or false"),
                },
                _ => {
                    if let Some(name) = key.strip_prefix("profile.") {
                        let mut parts = split_command(value);
                        if parts.is_empty() || name.trim().is_empty() {
                            bad("expected `profile.<name> = <command> [args...]`");
                        } else {
                            config.profiles.push(Profile {
                                name: name.trim().to_string(),
                                command: parts.remove(0),
                                args: parts,
                            });
                        }
                    } else {
                        config
                            .warnings
                            .push(format!("line {line_no}: unknown setting `{key}`"));
                    }
                }
            }
        }

        config
    }

    /// Read the config from disk. A missing file is not an error: it yields
    /// defaults, which are written back on the first save.
    pub fn load() -> (Self, Option<PathBuf>) {
        let Some(path) = config_path() else {
            return (Config::default(), None);
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => (Config::parse(&text), Some(path)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                (Config::default(), Some(path))
            }
            Err(err) => {
                let mut config = Config::default();
                config
                    .warnings
                    .push(format!("could not read {path:?}: {err}"));
                (config, Some(path))
            }
        }
    }

    fn managed_value(&self, key: &str) -> Option<String> {
        Some(match key {
            "shell" => self.shell.clone()?,
            "shell_args" => {
                if self.shell_args.is_empty() {
                    return None;
                }
                self.shell_args.join(" ")
            }
            "font_size" => format_number(self.font_size),
            "font_path" => self.font_path.clone()?,
            "theme" => self.theme.clone(),
            "tab_bar_auto_hide" => self.tab_bar_auto_hide.to_string(),
            "confirm_close" => self.confirm_close.to_string(),
            "scrollbar" => self.scrollbar.to_string(),
            "window_width" => format_number(self.window_width),
            "window_height" => format_number(self.window_height),
            "window_maximized" => self.window_maximized.to_string(),
            _ => return None,
        })
    }

    /// Render the config back to text, updating managed values in `existing`
    /// while leaving comments, key order and unknown keys untouched.
    pub fn render(&self, existing: Option<&str>) -> String {
        let source = existing.unwrap_or(TEMPLATE);
        let ends_with_newline = source.ends_with('\n') || existing.is_none();
        let mut written: Vec<&str> = Vec::new();
        let mut out: Vec<String> = Vec::new();

        for line in source.lines() {
            match split_entry(line) {
                Some((key, _)) if MANAGED_KEYS.contains(&key) && !written.contains(&key) => {
                    written.push(key);
                    match self.managed_value(key) {
                        Some(value) => out.push(format!("{key} = {value}")),
                        // Unset values stay in the file as a commented hint.
                        None => out.push(format!("# {key} =")),
                    }
                }
                _ => out.push(line.to_string()),
            }
        }

        let missing: Vec<String> = MANAGED_KEYS
            .iter()
            .filter(|key| !written.contains(*key))
            .filter_map(|key| {
                self.managed_value(key)
                    .map(|value| format!("{key} = {value}"))
            })
            .collect();
        if !missing.is_empty() {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push("# Added by myterm".to_string());
            out.extend(missing);
        }

        let mut text = out.join("\n");
        if ends_with_newline {
            text.push('\n');
        }
        text
    }

    /// Write the config to `path`, creating the parent directory if needed.
    pub fn save(&self, path: &PathBuf) -> std::io::Result<()> {
        let existing = std::fs::read_to_string(path).ok();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.render(existing.as_deref()))
    }

    /// The shell profiles offered in the new-tab menu: the configured default
    /// first, then any `profile.*` entries.
    pub fn menu_profiles(&self) -> Vec<Profile> {
        let default_command = self.shell.clone().unwrap_or_else(platform_default_shell);
        let default = Profile {
            name: display_name(&default_command),
            command: default_command,
            args: self.shell_args.clone(),
        };

        let mut profiles = vec![default];
        for profile in &self.profiles {
            if !profiles
                .iter()
                .any(|p| p.command == profile.command && p.args == profile.args)
            {
                profiles.push(profile.clone());
            }
        }
        profiles
    }

    /// The profile used for a plain "new tab".
    pub fn default_profile(&self) -> Profile {
        self.menu_profiles().remove(0)
    }
}

/// Trim the trailing `.0` from whole numbers so the file reads naturally.
fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < f32::EPSILON {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.1}")
    }
}

/// A short label for a shell command: `C:\...\powershell.exe` -> `powershell`.
pub fn display_name(command: &str) -> String {
    let base = command.rsplit(['/', '\\']).next().unwrap_or(command).trim();
    let stem = base.strip_suffix(".exe").unwrap_or(base);
    if stem.is_empty() {
        command.to_string()
    } else {
        stem.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
        assert!(config.warnings.is_empty());
        assert!(crate::theme::palette_by_name(&config.theme).is_some());
    }

    #[test]
    fn parses_every_managed_key() {
        let config = Config::parse(
            r#"
            # a comment
            shell = cmd.exe
            shell_args = /k "echo hi"
            font_size = 18.5
            font_path = C:\fonts\Cascadia Mono.ttf
            theme = gruvbox-dark
            tab_bar_auto_hide = yes
            confirm_close = 0
            scrollbar = off
            window_width = 1400
            window_height = 900
            window_maximized = true
            "#,
        );
        assert_eq!(config.shell.as_deref(), Some("cmd.exe"));
        assert_eq!(config.shell_args, vec!["/k", "echo hi"]);
        assert_eq!(config.font_size, 18.5);
        assert_eq!(
            config.font_path.as_deref(),
            Some(r"C:\fonts\Cascadia Mono.ttf")
        );
        assert_eq!(config.theme, "gruvbox-dark");
        assert!(config.tab_bar_auto_hide);
        assert!(!config.confirm_close);
        assert!(!config.scrollbar);
        assert_eq!(config.window_width, 1400.0);
        assert!(config.window_maximized);
        assert!(config.warnings.is_empty(), "{:?}", config.warnings);
    }

    #[test]
    fn bad_values_warn_and_keep_defaults() {
        let config = Config::parse("font_size = huge\ntheme = nope\nnonsense = 1\n");
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(config.theme, Config::default().theme);
        assert_eq!(config.warnings.len(), 3);
        assert!(config.warnings[0].contains("line 1"));
        assert!(config.warnings[2].contains("unknown setting"));
    }

    #[test]
    fn font_size_is_clamped() {
        assert_eq!(Config::parse("font_size = 900").font_size, MAX_FONT_SIZE);
        assert_eq!(Config::parse("font_size = 1").font_size, MIN_FONT_SIZE);
    }

    #[test]
    fn parses_profiles_with_quoted_paths() {
        let config = Config::parse(
            "profile.PowerShell = powershell.exe -NoLogo\n\
             profile.Dev Shell = \"C:\\Program Files\\pwsh\\pwsh.exe\" -NoLogo -File \"c:\\a b.ps1\"\n",
        );
        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profiles[0].name, "PowerShell");
        assert_eq!(config.profiles[0].command, "powershell.exe");
        assert_eq!(config.profiles[0].args, vec!["-NoLogo"]);
        assert_eq!(config.profiles[1].name, "Dev Shell");
        assert_eq!(
            config.profiles[1].command,
            r"C:\Program Files\pwsh\pwsh.exe"
        );
        assert_eq!(
            config.profiles[1].args,
            vec!["-NoLogo", "-File", r"c:\a b.ps1"]
        );
    }

    #[test]
    fn rendering_the_template_round_trips() {
        let config = Config {
            font_size: 16.0,
            theme: "gruvbox-dark".into(),
            window_width: 1234.0,
            ..Config::default()
        };

        let text = config.render(None);
        let reparsed = Config::parse(&text);
        assert!(reparsed.warnings.is_empty(), "{:?}", reparsed.warnings);
        assert_eq!(reparsed.font_size, 16.0);
        assert_eq!(reparsed.theme, "gruvbox-dark");
        assert_eq!(reparsed.window_width, 1234.0);
        assert_eq!(reparsed.shell, None);
    }

    #[test]
    fn rendering_preserves_comments_and_unknown_keys() {
        let existing = "# my notes\nfont_size = 12\nfuture_option = 3\n\n# trailing note\n";
        let mut config = Config::parse(existing);
        config.font_size = 20.0;

        let rendered = config.render(Some(existing));
        assert!(rendered.starts_with("# my notes\n"));
        assert!(rendered.contains("font_size = 20\n"));
        assert!(rendered.contains("future_option = 3"));
        assert!(rendered.contains("# trailing note"));
        assert!(!rendered.contains("font_size = 12"));
    }

    #[test]
    fn rendering_keeps_profiles_untouched() {
        let existing = "profile.Bash = /bin/bash -l\ntheme = myterm\n";
        let config = Config::parse(existing);
        let rendered = config.render(Some(existing));
        assert!(rendered.contains("profile.Bash = /bin/bash -l"));
        assert_eq!(Config::parse(&rendered).profiles.len(), 1);
    }

    #[test]
    fn unset_values_are_commented_out_on_save() {
        let existing = "shell = cmd.exe\n";
        let mut config = Config::parse(existing);
        config.shell = None;
        let rendered = config.render(Some(existing));
        assert!(rendered.contains("# shell ="), "{rendered}");
        assert_eq!(Config::parse(&rendered).shell, None);
    }

    #[test]
    fn command_splitting_handles_quotes() {
        assert_eq!(split_command("  "), Vec::<String>::new());
        assert_eq!(split_command("a b"), vec!["a", "b"]);
        assert_eq!(split_command("\"a b\" c"), vec!["a b", "c"]);
        assert_eq!(split_command("--msg=\"hi there\""), vec!["--msg=hi there"]);
        assert_eq!(split_command("\"\""), vec![""]);
    }

    #[test]
    fn menu_profiles_start_with_the_default_shell() {
        let config = Config::parse("shell = cmd.exe\nprofile.Bash = /bin/bash\n");
        let profiles = config.menu_profiles();
        assert_eq!(profiles[0].command, "cmd.exe");
        assert_eq!(profiles[0].name, "cmd");
        assert_eq!(profiles[1].name, "Bash");
        assert_eq!(config.default_profile().command, "cmd.exe");
    }

    #[test]
    fn menu_profiles_do_not_duplicate_the_default() {
        let config = Config::parse("shell = /bin/bash\nprofile.Bash = /bin/bash\n");
        assert_eq!(config.menu_profiles().len(), 1);
    }

    #[test]
    fn display_names_are_readable() {
        assert_eq!(display_name(r"C:\Windows\System32\cmd.exe"), "cmd");
        assert_eq!(display_name("/bin/bash"), "bash");
        assert_eq!(display_name("pwsh"), "pwsh");
    }

    #[test]
    fn numbers_render_without_noise() {
        assert_eq!(format_number(14.0), "14");
        assert_eq!(format_number(14.5), "14.5");
    }
}
