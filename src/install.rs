//! Finding, installing and opening the apps on this computer.
use crate::catalog::AppDef;
use crate::cfg::{home, State};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    /// version as the app itself reports it, else the release tag the launcher installed
    pub version: Option<String>,
}

fn user_home() -> PathBuf { home().parent().map(|p| p.to_path_buf()).unwrap_or_default() }

fn plist_version(app: &Path) -> Option<String> {
    let o = Command::new("plutil").args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"]).arg(app.join("Contents/Info.plist")).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && !s.is_empty()).then_some(s)
}

pub fn find(app: &AppDef, st: &State) -> Option<Found> {
    let rec = st.installed.get(&app.id);
    if cfg!(target_os = "macos") {
        let name = app.mac_app.as_ref()?;
        for base in [PathBuf::from("/Applications"), user_home().join("Applications")] {
            let p = base.join(name);
            if p.exists() { return Some(Found { version: plist_version(&p).or_else(|| rec.map(|r| r.tag.clone())), path: p }); }
        }
        None
    } else if cfg!(target_os = "windows") {
        if let Some(r) = rec { if Path::new(&r.path).exists() { return Some(Found { path: PathBuf::from(&r.path), version: Some(r.tag.clone()) }); } }
        let exe = app.win_exe.as_ref()?;
        let stem = Path::new(exe).file_stem()?.to_string_lossy().to_string();
        let mut roots = vec![];
        for k in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(v) = std::env::var_os(k) { let v = PathBuf::from(v); roots.push(if k == "LOCALAPPDATA" { v.join("Programs") } else { v }); }
        }
        for r in roots {
            for p in [r.join(&stem).join(exe), r.join(&app.name).join(exe)] { if p.exists() { return Some(Found { path: p, version: None }); } }
        }
        None
    } else { None }
}

/// is the app running right now? (replacing a running app fails on Windows and would confuse it on a Mac)
pub fn is_running(app: &AppDef) -> bool {
    if cfg!(target_os = "macos") {
        let Some(n) = &app.mac_app else { return false };
        Command::new("pgrep").args(["-f", &format!("{n}/Contents/MacOS")]).output().map(|o| o.status.success()).unwrap_or(false)
    } else if cfg!(target_os = "windows") {
        let Some(e) = &app.win_exe else { return false };
        Command::new("tasklist").args(["/FI", &format!("IMAGENAME eq {e}"), "/NH"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(&e.to_lowercase())).unwrap_or(false)
    } else { false }
}

pub fn open(f: &Found) -> Result<(), String> {
    let r = if cfg!(target_os = "macos") { Command::new("open").arg(&f.path).spawn() }
        else if cfg!(target_os = "windows") { Command::new("cmd").args(["/C", "start", ""]).arg(&f.path).spawn() }
        else { Command::new("xdg-open").arg(&f.path).spawn() };
    r.map(|_| ()).map_err(|e| format!("could not open: {e}"))
}

pub fn reveal(p: &Path) {
    if cfg!(target_os = "macos") { let _ = Command::new("open").arg("-R").arg(p).spawn(); }
    else if cfg!(target_os = "windows") { let _ = Command::new("explorer").arg(format!("/select,{}", p.display())).spawn(); }
}

pub(crate) fn run(c: &mut Command, what: &str) -> Result<String, String> {
    let o = c.output().map_err(|e| format!("{what}: {e}"))?;
    if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).to_string()) } else { Err(format!("{what} failed: {}", String::from_utf8_lossy(&o.stderr).trim())) }
}

pub fn downloads_dir() -> PathBuf { let d = user_home().join("Downloads"); if d.exists() { d } else { home().join("downloads") } }

/// install a downloaded file; returns the path of the installed app (or the saved file for an APK)
pub fn install(app: &AppDef, file: &Path) -> Result<PathBuf, String> {
    let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "apk" => {
            let dst = downloads_dir().join(file.file_name().unwrap());
            std::fs::create_dir_all(dst.parent().unwrap()).ok();
            if dst != file { std::fs::copy(file, &dst).map_err(|e| e.to_string())?; }
            Ok(dst)
        }
        "dmg" if cfg!(target_os = "macos") => {
            let mnt = home().join("mnt").join(&app.id);
            let _ = std::fs::create_dir_all(&mnt);
            run(Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-mountpoint"]).arg(&mnt).arg(file), "mounting the disk image")?;
            let res = (|| {
                let name = app.mac_app.clone().ok_or("the catalog has no app name for this app")?;
                let src = mnt.join(&name);
                if !src.exists() { return Err(format!("{name} is not inside the disk image")); }
                let dst = PathBuf::from("/Applications").join(&name);
                if dst.exists() { std::fs::remove_dir_all(&dst).map_err(|e| format!("could not replace the old copy: {e}"))?; }
                run(Command::new("ditto").arg(&src).arg(&dst), "copying the app")?;
                Ok(dst)
            })();
            let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mnt).output();
            res
        }
        "zip" if cfg!(target_os = "macos") => {
            let dst = PathBuf::from("/Applications");
            run(Command::new("ditto").args(["-x", "-k"]).arg(file).arg(&dst), "unpacking")?;
            Ok(dst.join(app.mac_app.clone().unwrap_or_default()))
        }
        "msi" if cfg!(target_os = "windows") => {
            run(Command::new("msiexec").args(["/i"]).arg(file).arg("/passive"), "Windows Installer")?;
            crate::install::find(app, &State::default()).map(|f| f.path).ok_or_else(|| "installed, but the app was not found in the usual folders".to_string())
        }
        "exe" if cfg!(target_os = "windows") => {
            let dir = home().join("apps").join(&app.id);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let dst = dir.join(file.file_name().unwrap());
            std::fs::copy(file, &dst).map_err(|e| e.to_string())?;
            Ok(dst)
        }
        "zip" if cfg!(target_os = "windows") => {
            let dir = home().join("apps").join(&app.id);
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            run(Command::new("tar").arg("-xf").arg(file).arg("-C").arg(&dir), "unpacking")?;
            let exe = app.win_exe.clone().unwrap_or_default();
            walk(&dir, &exe).ok_or_else(|| format!("{exe} was not found in the zip"))
        }
        _ => Err(format!("this launcher cannot install a .{ext} file on this system")),
    }
}

pub(crate) fn walk(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() { if let Some(f) = walk(&p, name) { return Some(f); } }
        else if p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false) { return Some(p); }
    }
    None
}
