//! Shell profiles, detected at startup (like the Windows Terminal new-tab
//! dropdown).

use std::path::PathBuf;
#[cfg(windows)]
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub shell: String,
    pub args: Vec<String>,
}

impl Profile {
    fn new(name: &str, shell: impl Into<String>, args: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            shell: shell.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }
}

/// All shells found on this machine. The first entry is the default; it can
/// be overridden with the `MYTERM_SHELL` environment variable.
pub fn detect() -> Vec<Profile> {
    let mut profiles = Vec::new();
    if let Ok(shell) = std::env::var("MYTERM_SHELL") {
        profiles.push(Profile::new("Default", shell, &[]));
    }
    profiles.extend(platform_profiles());
    if profiles.is_empty() {
        profiles.push(fallback());
    }
    profiles
}

#[cfg(windows)]
fn platform_profiles() -> Vec<Profile> {
    let mut found = vec![Profile::new("Windows PowerShell", "powershell.exe", &[])];
    if find_in_path("pwsh.exe").is_some() {
        found.push(Profile::new("PowerShell 7", "pwsh.exe", &[]));
    }
    found.push(Profile::new("Command Prompt", "cmd.exe", &[]));

    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    if Path::new(&system_root).join(r"System32\wsl.exe").is_file() {
        found.push(Profile::new("WSL", "wsl.exe", &[]));
    }

    let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let git_bash = Path::new(&program_files).join(r"Git\bin\bash.exe");
    if git_bash.is_file() {
        found.push(Profile::new(
            "Git Bash",
            git_bash.to_string_lossy(),
            &["--login", "-i"],
        ));
    }
    found
}

#[cfg(not(windows))]
fn platform_profiles() -> Vec<Profile> {
    let mut found = Vec::new();
    if let Ok(shell) = std::env::var("SHELL") {
        found.push(Profile::new("Login shell", shell, &[]));
    }
    for (name, bin) in [("bash", "bash"), ("zsh", "zsh"), ("fish", "fish")] {
        if let Some(path) = find_in_path(bin) {
            found.push(Profile::new(name, path.to_string_lossy(), &[]));
        }
    }
    found
}

fn fallback() -> Profile {
    if cfg!(windows) {
        Profile::new("Windows PowerShell", "powershell.exe", &[])
    } else {
        Profile::new("sh", "/bin/sh", &[])
    }
}

fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
}
