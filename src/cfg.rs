//! Settings and the record of what the launcher installed, in `~/.boxlauncher/`.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    let h = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    h.join(".boxlauncher")
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub dark: bool,
    pub table: bool,
    /// token for private repositories (kept in a file only you can read)
    pub token: String,
    /// also use the login of the GitHub CLI (`gh auth token`) for private repositories
    pub use_gh: bool,
}
impl Default for Config {
    fn default() -> Self { Config { dark: true, table: false, token: String::new(), use_gh: false } }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Installed {
    pub tag: String,
    pub path: String,
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    pub installed: HashMap<String, Installed>,
}

fn write_private(p: &PathBuf, text: &str) {
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(&home()));
    let _ = std::fs::write(p, text);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
    }
}

impl Config {
    pub fn load() -> Config { std::fs::read_to_string(home().join("config.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() }
    pub fn save(&self) { write_private(&home().join("config.json"), &serde_json::to_string_pretty(self).unwrap_or_default()); }
    /// the token to use for GitHub, if any
    pub fn github_token(&self) -> Option<String> {
        if !self.token.trim().is_empty() { return Some(self.token.trim().to_string()); }
        for k in ["GITHUB_TOKEN", "GH_TOKEN"] {
            if let Ok(v) = std::env::var(k) { if !v.trim().is_empty() { return Some(v.trim().to_string()); } }
        }
        if self.use_gh {
            if let Ok(o) = std::process::Command::new("gh").args(["auth", "token"]).output() {
                let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if o.status.success() && !t.is_empty() { return Some(t); }
            }
        }
        None
    }
}
impl State {
    pub fn load() -> State { std::fs::read_to_string(home().join("state.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() }
    pub fn save(&self) { write_private(&home().join("state.json"), &serde_json::to_string_pretty(self).unwrap_or_default()); }
}
