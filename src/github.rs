//! Releases on GitHub: what is the newest build, download it, and check it against the published SHA-256 sums.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Deserialize, Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

/// true when the answer means "this app is private and needs a license code"
pub fn is_locked(msg: &str) -> bool { msg.starts_with("not found") || msg.starts_with("repository not found") || msg.contains("was refused") }

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().user_agent("boxlauncher").timeout_connect(std::time::Duration::from_secs(15)).build()
}

/// the newest published release (pre-releases count, drafts do not)
pub fn latest(repo: &str, token: Option<&str>) -> Result<Release, String> {
    let mut rq = agent().get(&format!("https://api.github.com/repos/{repo}/releases?per_page=10")).set("Accept", "application/vnd.github+json");
    if let Some(t) = token { rq = rq.set("Authorization", &format!("Bearer {t}")); }
    match rq.call() {
        Ok(r) => {
            let list: Vec<Release> = r.into_json().map_err(|e| format!("unreadable answer: {e}"))?;
            list.into_iter().find(|x| !x.draft).ok_or_else(|| "no releases published yet".to_string())
        }
        Err(ureq::Error::Status(404, _)) => Err(if token.is_some() { "repository not found: this license code cannot open it".into() } else { "not found: needs a license code (Settings)".into() }),
        Err(ureq::Error::Status(401, _)) => Err("the license code was refused".into()),
        Err(ureq::Error::Status(403, _)) => Err("GitHub refused the request (too many requests?), try again later".into()),
        Err(ureq::Error::Status(c, _)) => Err(format!("GitHub answered {c}")),
        Err(e) => Err(format!("no connection: {e}")),
    }
}

/// the README of a repository as markdown text
pub fn readme(repo: &str, token: Option<&str>) -> Result<String, String> {
    let mut rq = agent().get(&format!("https://api.github.com/repos/{repo}/readme")).set("Accept", "application/vnd.github.raw+json");
    if let Some(t) = token { rq = rq.set("Authorization", &format!("Bearer {t}")); }
    match rq.call() {
        Ok(r) => { let mut s = String::new(); r.into_reader().take(1 << 20).read_to_string(&mut s).map_err(|e| e.to_string())?; Ok(s) }
        Err(ureq::Error::Status(404, _)) => Err(if token.is_some() { "repository not found: this license code cannot open it".into() } else { "not found: needs a license code (Settings)".into() }),
        Err(ureq::Error::Status(401, _)) => Err("the license code was refused".into()),
        Err(ureq::Error::Status(c, _)) => Err(format!("GitHub answered {c}")),
        Err(e) => Err(format!("no connection: {e}")),
    }
}

fn get_asset(a: &Asset, token: Option<&str>) -> Result<Box<dyn Read + Send + Sync>, String> {
    let mut rq = agent().get(&a.url).set("Accept", "application/octet-stream");
    if let Some(t) = token { rq = rq.set("Authorization", &format!("Bearer {t}")); }
    rq.call().map(|r| r.into_reader()).map_err(|e| format!("download failed: {e}"))
}

/// download to `dest`, calling `progress(done, total)`; returns the SHA-256 (hex) of what was written
pub fn download(a: &Asset, token: Option<&str>, dest: &Path, mut progress: impl FnMut(u64, u64)) -> Result<String, String> {
    let mut rd = get_asset(a, token)?;
    if let Some(d) = dest.parent() { let _ = std::fs::create_dir_all(d); }
    let part = dest.with_extension("part");
    let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let (mut h, mut buf, mut done) = (Sha256::new(), vec![0u8; 64 * 1024], 0u64);
    let mut last = std::time::Instant::now();
    loop {
        let n = rd.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 { break; }
        h.update(&buf[..n]); f.write_all(&buf[..n]).map_err(|e| e.to_string())?; done += n as u64;
        if last.elapsed().as_millis() > 100 { progress(done, a.size.max(done)); last = std::time::Instant::now(); }
    }
    drop(f);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    progress(done, done);
    Ok(hex::encode(h.finalize()))
}

pub fn fetch_text(a: &Asset, token: Option<&str>) -> Result<String, String> {
    let mut s = String::new();
    get_asset(a, token)?.take(1 << 20).read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

/// expected hash of `file` from the text of a SHA256SUMS file ("<hash>  <name>" per line)
pub fn expected_hash(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n.trim_start_matches('*') == file && h.len() == 64).then(|| h.to_lowercase())
    })
}

pub fn sums_asset(r: &Release) -> Option<&Asset> {
    r.assets.iter().find(|a| { let n = a.name.to_lowercase(); n.starts_with("sha256sums") || n == "checksums.txt" })
}

/// which asset to install on `os` ("macos", "windows", "android"), best first.
/// Windows prefers the portable zip: it needs no administrator rights and works on PCs that do not allow .msi installers.
pub fn pick_asset<'a>(r: &'a Release, os: &str) -> Option<&'a Asset> {
    let ext_rank: &[&str] = match os { "macos" => &[".dmg", ".pkg"], "windows" => &[".zip", ".exe", ".msi"], "android" => &[".apk"], _ => &[] };
    for e in ext_rank {
        let hit = r.assets.iter().find(|a| {
            let n = a.name.to_lowercase();
            n.ends_with(e) && match os { "windows" => *e != ".zip" || n.contains("win"), "macos" => true, _ => true }
        });
        if hit.is_some() { return hit; }
    }
    if os == "macos" { return r.assets.iter().find(|a| { let n = a.name.to_lowercase(); n.ends_with(".zip") && (n.contains("mac") || n.contains("darwin")) }); }
    None
}

/// "v1.2" -> 1.2.0
pub fn parse_version(s: &str) -> Option<semver::Version> {
    let t = s.trim().trim_start_matches(['v', 'V']);
    if let Ok(v) = semver::Version::parse(t) { return Some(v); }
    let core: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut p: Vec<&str> = core.split('.').filter(|x| !x.is_empty()).collect();
    if p.is_empty() { return None; }
    while p.len() < 3 { p.push("0"); }
    semver::Version::parse(&p[..3].join(".")).ok()
}
pub fn newer(latest: &str, installed: &str) -> bool {
    match (parse_version(latest), parse_version(installed)) { (Some(a), Some(b)) => a > b, _ => latest.trim_start_matches('v') != installed.trim_start_matches('v') }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rel(names: &[&str]) -> Release { Release { tag_name: "v1".into(), published_at: None, draft: false, prerelease: false, assets: names.iter().map(|n| Asset { name: n.to_string(), size: 1, url: String::new() }).collect() } }
    #[test]
    fn picks_the_right_installer() {
        let r = rel(&["DriveBox-1.0.2-x64.msi", "DriveBox-1.0.2-win-x64-portable.zip", "SHA256SUMS.txt"]);
        assert_eq!(pick_asset(&r, "windows").unwrap().name, "DriveBox-1.0.2-win-x64-portable.zip");
        assert_eq!(pick_asset(&rel(&["DriveBox-1.0.2-x64.msi"]), "windows").unwrap().name, "DriveBox-1.0.2-x64.msi");
        assert!(pick_asset(&r, "macos").is_none());
        assert_eq!(sums_asset(&r).unwrap().name, "SHA256SUMS.txt");
        assert_eq!(pick_asset(&rel(&["PortalAgent.exe"]), "windows").unwrap().name, "PortalAgent.exe");
        let nb = rel(&["NodeBox-0.1.0-mac-arm64.dmg", "NodeBox-0.1.0-mac-arm64.zip", "NodeBox-0.1.0-win-x64.msi", "NodeBox-0.1.0-win-x64.zip", "SHA256SUMS.txt"]);
        assert_eq!(pick_asset(&nb, "macos").unwrap().name, "NodeBox-0.1.0-mac-arm64.dmg");
        assert_eq!(pick_asset(&nb, "windows").unwrap().name, "NodeBox-0.1.0-win-x64.zip");
        assert_eq!(pick_asset(&rel(&["x-1.0.apk"]), "android").unwrap().name, "x-1.0.apk");
        assert_eq!(pick_asset(&rel(&["App-1.0-mac.zip", "App.dmg"]), "macos").unwrap().name, "App.dmg");
    }
    #[test]
    fn reads_sums() {
        let h = "a".repeat(64);
        assert_eq!(expected_hash(&format!("{h}  a.msi\n{}  *b.zip\n", "b".repeat(64)), "b.zip").unwrap(), "b".repeat(64));
        assert!(expected_hash(&format!("{h}  a.msi"), "c.msi").is_none());
    }
    #[test]
    fn compares_versions() {
        assert!(newer("v1.0.1", "1.0.0")); assert!(!newer("v1.0.0", "1.0")); assert!(newer("v1.5.4", "v1.5.3"));
        assert!(newer("v1.0.1", "1.0.1-alpha.3")); assert!(newer("v1.0.1-alpha.3", "1.0.1-alpha.2"));
    }
    /// real network: downloads a public release and checks it against its SHA256SUMS (run with `cargo test -- --ignored`)
    #[test]
    #[ignore]
    fn real_download_is_verified() {
        let r = latest("mightyy-duck/drivebox", None).unwrap();
        let a = pick_asset(&r, "windows").unwrap();
        let sums = fetch_text(sums_asset(&r).unwrap(), None).unwrap();
        let dest = std::env::temp_dir().join("boxlauncher-test").join(&a.name);
        let mut last = 0.0;
        let got = download(a, None, &dest, |d, t| last = d as f64 / t as f64).unwrap();
        assert_eq!(Some(got), expected_hash(&sums, &a.name));
        assert!(last > 0.99);
        let _ = std::fs::remove_file(dest);
    }
}
