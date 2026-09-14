//! Runtime configuration, read from the environment at startup.

use std::path::PathBuf;

/// Program to launch in each new tab.
pub const SHELL_ENV: &str = "MYTERM_SHELL";
/// Arguments passed to that program, split on whitespace with quoting.
pub const SHELL_ARGS_ENV: &str = "MYTERM_SHELL_ARGS";
/// Terminal font size, in points.
pub const FONT_SIZE_ENV: &str = "MYTERM_FONT_SIZE";

pub const DEFAULT_FONT_SIZE: f32 = 14.0;
const MIN_FONT_SIZE: f32 = 6.0;
const MAX_FONT_SIZE: f32 = 72.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub shell: String,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub font_size: f32,
}

impl Config {
    pub fn from_env() -> Self {
        Self::resolve(
            env(SHELL_ENV),
            env(SHELL_ARGS_ENV),
            env(FONT_SIZE_ENV),
            std::env::current_dir().ok(),
        )
    }

    fn resolve(
        shell: Option<String>,
        args: Option<String>,
        font_size: Option<String>,
        working_directory: Option<PathBuf>,
    ) -> Self {
        Self {
            shell: shell.unwrap_or_else(default_shell),
            args: args.as_deref().map(split_args).unwrap_or_default(),
            working_directory,
            font_size: font_size
                .as_deref()
                .and_then(parse_font_size)
                .unwrap_or(DEFAULT_FONT_SIZE),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::resolve(None, None, None, None)
    }
}

/// Reads an environment variable, treating unset and blank the same so that
/// `MYTERM_SHELL=` does not try to launch the empty string.
fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        env("SHELL").unwrap_or_else(|| "/bin/sh".to_string())
    }
}

/// Splits a command line into arguments on whitespace, honouring single and
/// double quotes so that paths with spaces survive.
pub fn split_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut in_arg = false;

    for c in raw.chars() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_arg = true;
            }
            None if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            None => {
                current.push(c);
                in_arg = true;
            }
        }
    }

    if in_arg {
        args.push(current);
    }

    args
}

/// Parses a font size, ignoring anything unusable and clamping the rest to a
/// range that still renders.
fn parse_font_size(raw: &str) -> Option<f32> {
    let size = raw.trim().parse::<f32>().ok()?;
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    Some(size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_usable() {
        let config = Config::resolve(None, None, None, None);
        assert!(!config.shell.is_empty());
        assert!(config.args.is_empty());
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
    }

    #[test]
    fn the_shell_and_its_arguments_are_taken_from_the_environment() {
        let config = Config::resolve(
            Some("cmd.exe".to_string()),
            Some("/k echo hi".to_string()),
            None,
            None,
        );
        assert_eq!(config.shell, "cmd.exe");
        assert_eq!(config.args, ["/k", "echo", "hi"]);
    }

    #[test]
    fn quoted_arguments_stay_in_one_piece() {
        assert_eq!(
            split_args(r#"-c "echo hello world""#),
            ["-c", "echo hello world"]
        );
        assert_eq!(split_args("-c 'echo hi'"), ["-c", "echo hi"]);
        assert_eq!(
            split_args(r#"--path "C:\Program Files\x""#),
            ["--path", r"C:\Program Files\x"]
        );
    }

    #[test]
    fn argument_splitting_handles_the_awkward_cases() {
        assert!(split_args("").is_empty());
        assert!(split_args("   \t ").is_empty());
        assert_eq!(split_args("  a   b  "), ["a", "b"]);
        // An explicitly empty argument is a real argument.
        assert_eq!(split_args(r#"a "" b"#), ["a", "", "b"]);
        // An unterminated quote takes the rest of the line rather than
        // dropping it.
        assert_eq!(split_args(r#"a "b c"#), ["a", "b c"]);
    }

    #[test]
    fn font_size_is_parsed_and_clamped() {
        assert_eq!(parse_font_size("18"), Some(18.0));
        assert_eq!(parse_font_size(" 10.5 "), Some(10.5));
        assert_eq!(parse_font_size("1"), Some(MIN_FONT_SIZE));
        assert_eq!(parse_font_size("9000"), Some(MAX_FONT_SIZE));
    }

    #[test]
    fn an_unusable_font_size_falls_back_to_the_default() {
        for raw in ["", "big", "0", "-3", "NaN", "inf"] {
            let config = Config::resolve(None, None, Some(raw.to_string()), None);
            assert_eq!(
                config.font_size, DEFAULT_FONT_SIZE,
                "{raw:?} should not change the font size"
            );
        }
    }
}
