//! The list of apps the launcher manages. A built-in default, which `~/.boxlauncher/catalog.json` can replace.
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct AppDef {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub accent: [u8; 3],
    #[serde(default)]
    pub icon: Option<String>,
    /// "owner/name" of the GitHub repository whose releases hold the installers
    #[serde(default)]
    pub repo: Option<String>,
    /// "macos", "windows", "android"
    pub platforms: Vec<String>,
    #[serde(default)]
    pub mac_app: Option<String>,
    #[serde(default)]
    pub win_exe: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

const DEFAULT: &str = include_str!("../catalog.json");

pub fn load() -> Vec<AppDef> {
    if let Ok(t) = std::fs::read_to_string(crate::cfg::home().join("catalog.json")) {
        if let Ok(v) = serde_json::from_str::<Vec<AppDef>>(&t) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    serde_json::from_str(DEFAULT).expect("built-in catalog is valid")
}

pub fn os() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else if cfg!(target_os = "windows") { "windows" } else { "linux" }
}

impl AppDef {
    pub fn runs_here(&self) -> bool {
        self.platforms.iter().any(|p| p == os())
    }
    pub fn is_android(&self) -> bool {
        self.platforms.iter().any(|p| p == "android")
    }
    pub fn letter(&self) -> String {
        self.name.chars().next().unwrap_or('?').to_uppercase().to_string()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_catalog_parses() {
        let v: Vec<super::AppDef> = serde_json::from_str(super::DEFAULT).unwrap();
        assert!(v.iter().any(|a| a.id == "drivebox" && a.repo.as_deref() == Some("mightyy-duck/drivebox")));
        assert_eq!(v.len(), 5);
    }
}
