//! Updating Box Launcher itself from its own GitHub release.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const REPO: &str = "mightyy-duck/boxlauncher";

/// the .app bundle this copy runs from (macOS)
fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().map_or(false, |e| e == "app")).map(|p| p.to_path_buf())
}

/// can this copy replace itself? (not when run from a build folder or from a read-only place)
pub fn can_update() -> Result<(), String> {
    if cfg!(target_os = "macos") {
        if std::env::consts::ARCH != "aarch64" { return Err("self-update is built for Apple silicon Macs".into()); }
        bundle().map(|_| ()).ok_or_else(|| "not running from an .app".to_string())
    } else if cfg!(target_os = "windows") { Ok(()) } else { Err("self-update is not available on this system".into()) }
}

/// Windows leaves the old exe next to the new one for one run: remove it
pub fn cleanup_old() {
    if cfg!(target_os = "windows") {
        if let Ok(exe) = std::env::current_exe() { let _ = std::fs::remove_file(exe.with_extension("exe.old")); }
    }
}

/// Replace the running copy with the one inside `zip`, start the new one and quit this one.
/// Only returns when something went wrong (and then nothing was changed).
pub fn apply(zip: &Path) -> Result<(), String> {
    let work = crate::cfg::home().join("selfupdate");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") {
        crate::install::run(Command::new("ditto").args(["-x", "-k"]).arg(zip).arg(&work), "unpacking")?;
        let new_app = std::fs::read_dir(&work).map_err(|e| e.to_string())?.flatten().map(|e| e.path())
            .find(|p| p.extension().map_or(false, |e| e == "app")).ok_or("the download holds no app")?;
        let app = bundle().ok_or("not running from an .app")?;
        if app.components().count() < 3 { return Err("unexpected install location".into()); }
        // a detached shell waits until this process has ended, swaps the bundle and opens the new one
        let script = format!("while kill -0 {} 2>/dev/null; do sleep 0.3; done; rm -rf \"$APP\" && ditto \"$NEW\" \"$APP\" && xattr -cr \"$APP\"; open \"$APP\"", std::process::id());
        Command::new("sh").arg("-c").arg(script).env("APP", &app).env("NEW", &new_app)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("could not start the swap: {e}"))?;
        std::process::exit(0);
    } else if cfg!(target_os = "windows") {
        crate::install::run(Command::new("tar").arg("-xf").arg(zip).arg("-C").arg(&work), "unpacking")?;
        let new_exe = crate::install::walk(&work, "boxlauncher.exe").ok_or("boxlauncher.exe was not found in the download")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let old = exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old);
        // Windows lets a running exe be renamed, not overwritten
        std::fs::rename(&exe, &old).map_err(|e| format!("cannot replace this copy ({e}). If it is installed in Program Files, install the new .msi instead."))?;
        if let Err(e) = std::fs::copy(&new_exe, &exe) { let _ = std::fs::rename(&old, &exe); return Err(format!("could not write the new version: {e}")); }
        Command::new(&exe).spawn().map_err(|e| format!("updated, but could not restart: {e}"))?;
        std::process::exit(0);
    }
    Err("self-update is not available on this system".into())
}
