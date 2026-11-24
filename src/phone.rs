//! Which phone is around, and is the app on it? iPhone through Xcode's `devicectl`, Android through `adb`.
use crate::cfg;
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum On { Yes, No, /// phone asleep / out of reach: cannot look
    Unknown }

#[derive(Clone, Debug)]
pub struct Phone { pub id: String, pub checking: bool, pub name: String, pub model: String, pub ios: bool, pub reachable: bool, pub has_app: On }

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None }
}

/// the paired iPhones, fast: no question is asked to the phones yet
fn ios_phones() -> Vec<Phone> {
    if !cfg!(target_os = "macos") { return vec![]; }
    let dir = cfg::home().join("tmp"); let _ = std::fs::create_dir_all(&dir);
    let f = dir.join("devices.json");
    if run("xcrun", &["devicectl", "list", "devices", "--json-output", f.to_str().unwrap_or("")]).is_none() { return vec![]; }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&f).unwrap_or_default()) else { return vec![] };
    let mut out = vec![];
    for d in v["result"]["devices"].as_array().cloned().unwrap_or_default() {
        if d["hardwareProperties"]["platform"].as_str() != Some("iOS") { continue; }
        let id = d["identifier"].as_str().unwrap_or("").to_string();
        let name = d["deviceProperties"]["name"].as_str().unwrap_or("iPhone").to_string();
        let model = d["hardwareProperties"]["marketingName"].as_str().or(d["hardwareProperties"]["productType"].as_str()).unwrap_or("iPhone").to_string();
        // "unavailable" = known but switched off / far away: no point in asking it
        let gone = d["connectionProperties"]["tunnelState"].as_str() == Some("unavailable");
        out.push(Phone { id, checking: !gone, name, model, ios: true, reachable: false, has_app: On::Unknown });
    }
    out
}

/// ask one iPhone whether the app is on it (a sleeping phone can take ~15 s to give up)
pub fn ios_has_app(id: &str, bundle: &str) -> (On, bool) {
    let g = cfg::home().join("tmp").join(format!("apps-{id}.json"));
    let _ = std::fs::remove_file(&g);
    if run("xcrun", &["devicectl", "device", "info", "apps", "--device", id, "--bundle-id", bundle, "--json-output", g.to_str().unwrap_or("")]).is_none() { return (On::Unknown, false); }
    let n = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&g).unwrap_or_default()).ok().and_then(|j| j["result"]["apps"].as_array().map(|a| a.len())).unwrap_or(0);
    (if n > 0 { On::Yes } else { On::No }, true)
}

fn android_phones(pkg: Option<&str>) -> Vec<Phone> {
    let adb = ["adb", "/opt/homebrew/bin/adb", "/usr/local/bin/adb"].into_iter().find(|a| run(a, &["version"]).is_some());
    let Some(adb) = adb else { return vec![] };
    let list = run(adb, &["devices"]).unwrap_or_default();
    list.lines().skip(1).filter_map(|l| { let mut p = l.split_whitespace(); let id = p.next()?; (p.next()? == "device").then(|| id.to_string()) }).map(|id| {
        let has_app = match pkg { Some(p) => if run(adb, &["-s", &id, "shell", "pm", "list", "packages", p]).map_or(false, |o| o.lines().any(|l| l.trim() == format!("package:{p}"))) { On::Yes } else { On::No }, None => On::Unknown };
        Phone { id: id.clone(), checking: false, name: id, model: "Android phone".into(), ios: false, reachable: true, has_app }
    }).collect()
}

pub fn detect(android_pkg: Option<&str>) -> Vec<Phone> {
    let mut v = ios_phones();
    v.extend(android_phones(android_pkg));
    v
}
