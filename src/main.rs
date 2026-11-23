mod catalog;
mod cfg;
mod github;
mod install;

use catalog::AppDef;
use cfg::{Config, Installed, State};
use eframe::egui::{self, Align, Align2, Color32, FontId, RichText, Sense, Stroke, TextureHandle, Vec2};
use github::Release;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

const ICON_LAUNCHER: &[u8] = include_bytes!("../assets/launcher.png");
const ICON_PAPERBOX: &[u8] = include_bytes!("../assets/paperbox.png");
const ICON_NODEBOX: &[u8] = include_bytes!("../assets/nodebox.png");
const ICON_LINKEDBOX: &[u8] = include_bytes!("../assets/linkedbox.png");
const ICON_DRIVEBOX: &[u8] = include_bytes!("../assets/drivebox.png");

enum Msg {
    Release(String, Result<Release, String>),
    Progress(String, f32, String),
    Done(String, Result<(String, PathBuf, String), String>),
}
#[derive(Clone)]
struct Tx(Sender<Msg>, egui::Context);
impl Tx {
    fn send(&self, m: Msg) { let _ = self.0.send(m); self.1.request_repaint(); }
}

enum Fetch { None, Loading, Ok(Release), Err(String) }
struct Job { frac: f32, text: String }
struct Row {
    app: AppDef,
    found: Option<install::Found>,
    fetch: Fetch,
    job: Option<Job>,
    /// (is_error, text) shown on the card until the next action
    notice: Option<(bool, String)>,
}
#[derive(Clone, Copy, PartialEq)]
enum Status { Update, Installed, NotInstalled, NotPublished, OtherOs, Android, Problem }
impl Row {
    fn latest_tag(&self) -> Option<&str> { if let Fetch::Ok(r) = &self.fetch { Some(&r.tag_name) } else { None } }
    fn status(&self) -> Status {
        if self.app.is_android() { return if matches!(self.fetch, Fetch::Err(_)) { Status::Problem } else { Status::Android }; }
        if !self.app.runs_here() { return Status::OtherOs; }
        if let Some(f) = &self.found {
            if let (Some(l), Some(v)) = (self.latest_tag(), &f.version) { if github::newer(l, v) { return Status::Update; } }
            return Status::Installed;
        }
        if self.app.repo.is_none() { return Status::NotPublished; }
        if matches!(self.fetch, Fetch::Err(_)) { Status::Problem } else { Status::NotInstalled }
    }
}
fn status_label(s: Status, app: &AppDef) -> (String, Color32) {
    match s {
        Status::Update => ("Update available".into(), Color32::from_rgb(255, 176, 112)),
        Status::Installed => ("Installed".into(), Color32::from_rgb(76, 217, 163)),
        Status::NotInstalled => ("Not installed".into(), Color32::from_rgb(160, 160, 170)),
        Status::NotPublished => ("Not published yet".into(), Color32::from_rgb(160, 160, 170)),
        Status::OtherOs => (format!("{} only", app.platforms.iter().map(|p| match p.as_str() { "windows" => "Windows", "macos" => "macOS", o => o }).collect::<Vec<_>>().join(" / ")), Color32::from_rgb(160, 160, 170)),
        Status::Android => ("Android app".into(), Color32::from_rgb(185, 164, 255)),
        Status::Problem => ("Cannot check".into(), Color32::from_rgb(255, 84, 112)),
    }
}

enum Act { Install(usize), Open(usize), Reveal(usize) }

struct App {
    cfg: Config,
    state: State,
    rows: Vec<Row>,
    tex: HashMap<String, TextureHandle>,
    logo: TextureHandle,
    tx: Tx,
    rx: Receiver<Msg>,
    settings: bool,
    sort: usize,
    was_focused: bool,
    shot: Option<(String, std::time::Instant)>,
}

fn decode(bytes: &[u8]) -> egui::ColorImage {
    let im = image::load_from_memory(bytes).expect("icon").to_rgba8();
    egui::ColorImage::from_rgba_unmultiplied([im.width() as usize, im.height() as usize], im.as_raw())
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<String>) -> App {
        let (tx, rx) = channel();
        let cfg = Config::load();
        let state = State::load();
        let mut tex = HashMap::new();
        tex.insert("paperbox".to_string(), cc.egui_ctx.load_texture("paperbox", decode(ICON_PAPERBOX), Default::default()));
        tex.insert("nodebox".to_string(), cc.egui_ctx.load_texture("nodebox", decode(ICON_NODEBOX), Default::default()));
        tex.insert("linkedbox".to_string(), cc.egui_ctx.load_texture("linkedbox", decode(ICON_LINKEDBOX), Default::default()));
        tex.insert("drivebox".to_string(), cc.egui_ctx.load_texture("drivebox", decode(ICON_DRIVEBOX), Default::default()));
        let logo = cc.egui_ctx.load_texture("logo", decode(ICON_LAUNCHER), Default::default());
        let rows = catalog::load().into_iter().map(|app| Row { found: install::find(&app, &state), fetch: if app.repo.is_some() { Fetch::Loading } else { Fetch::None }, app, job: None, notice: None }).collect();
        let mut a = App { cfg, state, rows, tex, logo, tx: Tx(tx, cc.egui_ctx.clone()), rx, settings: false, sort: 0, was_focused: true, shot: shot.map(|p| (p, std::time::Instant::now())) };
        style(&cc.egui_ctx, a.cfg.dark);
        a.refresh_releases();
        a
    }
    fn refresh_releases(&mut self) {
        let token = self.cfg.github_token();
        for r in &mut self.rows {
            if let Some(repo) = r.app.repo.clone() {
                r.fetch = Fetch::Loading;
                let (tx, id, tk) = (self.tx.clone(), r.app.id.clone(), token.clone());
                std::thread::spawn(move || { let res = github::latest(&repo, tk.as_deref()); tx.send(Msg::Release(id, res)); });
            }
        }
    }
    fn refresh_installed(&mut self) { for r in &mut self.rows { r.found = install::find(&r.app, &self.state); } }
    fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Release(id, res) => if let Some(r) = self.rows.iter_mut().find(|r| r.app.id == id) { r.fetch = match res { Ok(x) => Fetch::Ok(x), Err(e) => Fetch::Err(e) }; },
                Msg::Progress(id, f, t) => if let Some(r) = self.rows.iter_mut().find(|r| r.app.id == id) { r.job = Some(Job { frac: f, text: t }); },
                Msg::Done(id, res) => {
                    let mut st = None;
                    if let Some(r) = self.rows.iter_mut().find(|r| r.app.id == id) {
                        r.job = None;
                        match res {
                            Ok((tag, path, note)) => { r.notice = Some((false, note)); st = Some((tag, path)); }
                            Err(e) => r.notice = Some((true, e)),
                        }
                    }
                    if let Some((tag, path)) = st {
                        if !path.to_string_lossy().to_lowercase().ends_with(".apk") { self.state.installed.insert(id, Installed { tag, path: path.to_string_lossy().to_string() }); self.state.save(); }
                        self.refresh_installed();
                    }
                }
            }
        }
    }
    fn act(&mut self, a: Act) {
        match a {
            Act::Open(i) => { let r = &mut self.rows[i]; r.notice = None; if let Some(f) = &r.found { if let Err(e) = install::open(f) { r.notice = Some((true, e)); } } }
            Act::Reveal(i) => { if let Some(f) = &self.rows[i].found { install::reveal(&f.path); } }
            Act::Install(i) => {
                let r = &mut self.rows[i];
                let Fetch::Ok(rel) = &r.fetch else { return };
                r.notice = None; r.job = Some(Job { frac: 0.0, text: "Starting…".into() });
                let (app, rel, tk, tx) = (r.app.clone(), rel.clone(), self.cfg.github_token(), self.tx.clone());
                std::thread::spawn(move || { let id = app.id.clone(); let res = install_job(&app, &rel, tk.as_deref(), &tx); tx.send(Msg::Done(id, res)); });
            }
        }
    }
}

/// download, verify against the published SHA-256 sums, install
fn install_job(app: &AppDef, rel: &Release, token: Option<&str>, tx: &Tx) -> Result<(String, PathBuf, String), String> {
    let os = if app.is_android() { "android" } else { catalog::os() };
    let asset = github::pick_asset(rel, os).ok_or_else(|| format!("release {} has no installer for {os}", rel.tag_name))?;
    let dest = cfg::home().join("downloads").join(&app.id).join(&asset.name);
    let id = app.id.clone();
    let mb = |b: u64| b as f64 / 1_048_576.0;
    let got = github::download(asset, token, &dest, |d, t| tx.send(Msg::Progress(id.clone(), d as f32 / t.max(1) as f32, format!("Downloading {:.1} / {:.1} MB", mb(d), mb(t)))))?;
    tx.send(Msg::Progress(id.clone(), 1.0, "Checking the checksum…".into()));
    let note = match github::sums_asset(rel) {
        Some(s) => {
            let text = github::fetch_text(s, token)?;
            match github::expected_hash(&text, &asset.name) {
                Some(want) if want == got => "Checksum verified (SHA-256).".to_string(),
                Some(_) => { let _ = std::fs::remove_file(&dest); return Err("The download does not match its published SHA-256: it was deleted and nothing was installed.".into()); }
                None => "Installed (the checksum file does not list this file).".to_string(),
            }
        }
        None => "Installed. This release publishes no checksum, so it could not be verified.".to_string(),
    };
    tx.send(Msg::Progress(id.clone(), 1.0, "Installing…".into()));
    let path = install::install(app, &dest)?;
    let note = if app.is_android() { format!("Saved to {}. {note} Send it to the phone and install it there.", path.display()) } else { format!("Installed {}. {note}", rel.tag_name) };
    Ok((rel.tag_name.clone(), path, note))
}

fn palette(dark: bool) -> (Color32, Color32, Color32, Color32, Color32) {
    // background, panel, line, text, dim
    if dark { (Color32::from_rgb(11, 11, 12), Color32::from_rgb(19, 19, 21), Color32::from_rgb(46, 46, 52), Color32::from_rgb(236, 236, 239), Color32::from_rgb(142, 142, 152)) }
    else { (Color32::from_rgb(251, 249, 247), Color32::from_rgb(255, 255, 255), Color32::from_rgb(226, 219, 213), Color32::from_rgb(35, 31, 32), Color32::from_rgb(122, 113, 109)) }
}
fn style(ctx: &egui::Context, dark: bool) {
    let (bg, panel, line, text, _) = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = bg; v.window_fill = panel; v.extreme_bg_color = if dark { Color32::from_rgb(25, 25, 28) } else { Color32::from_rgb(246, 242, 239) };
    v.override_text_color = Some(text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
    v.widgets.inactive.bg_fill = if dark { Color32::from_rgb(27, 27, 31) } else { Color32::WHITE };
    v.widgets.inactive.weak_bg_fill = v.widgets.inactive.bg_fill;
    v.widgets.hovered.weak_bg_fill = if dark { Color32::from_rgb(38, 38, 44) } else { Color32::from_rgb(243, 238, 234) };
    v.selection.bg_fill = Color32::from_rgb(255, 123, 98).gamma_multiply(0.55);
    ctx.set_visuals(v);
    ctx.style_mut(|s| { s.spacing.item_spacing = Vec2::new(10.0, 8.0); s.spacing.button_padding = Vec2::new(14.0, 7.0); });
}

fn accent(app: &AppDef) -> Color32 { Color32::from_rgb(app.accent[0], app.accent[1], app.accent[2]) }
fn tile(ui: &mut egui::Ui, app: &AppDef, tex: Option<&TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if let Some(t) = tex { egui::Image::new(t).corner_radius((size * 0.24) as u8).paint_at(ui, rect); }
    else {
        ui.painter().rect_filled(rect, (size * 0.24) as u8, accent(app));
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, app.letter(), FontId::proportional(size * 0.5), Color32::from_rgb(24, 22, 24));
    }
}
fn chip(ui: &mut egui::Ui, text: &str, c: Color32) {
    egui::Frame::new().fill(c.gamma_multiply(0.16)).corner_radius(10).inner_margin(egui::Margin::symmetric(9, 3)).show(ui, |ui| { ui.label(RichText::new(text).color(c).size(12.5).strong()); });
}
fn primary(text: &str, c: Color32) -> egui::Button<'static> { egui::Button::new(RichText::new(text.to_string()).color(Color32::from_rgb(24, 22, 24)).strong()).fill(c) }
fn date(r: &Release) -> String { r.published_at.clone().unwrap_or_default().chars().take(10).collect() }

/// the main button(s) for one app
fn buttons(ui: &mut egui::Ui, i: usize, r: &Row, acts: &mut Vec<Act>) {
    let c = accent(&r.app);
    if r.job.is_some() { ui.add_enabled(false, egui::Button::new("Working…")); return; }
    let can = matches!(r.fetch, Fetch::Ok(_));
    match r.status() {
        Status::Installed => { if ui.add(primary("Open", c)).clicked() { acts.push(Act::Open(i)); } if ui.button("Show").clicked() { acts.push(Act::Reveal(i)); } }
        Status::Update => { if ui.add(primary("Update", c)).clicked() { acts.push(Act::Install(i)); } if ui.button("Open").clicked() { acts.push(Act::Open(i)); } }
        Status::NotInstalled => { if ui.add_enabled(can, primary("Install", c)).clicked() { acts.push(Act::Install(i)); } }
        Status::Android => { if ui.add_enabled(can, primary("Download APK", c)).clicked() { acts.push(Act::Install(i)); } }
        Status::Problem => { ui.add_enabled(false, egui::Button::new("Unavailable")); }
        Status::NotPublished | Status::OtherOs => { ui.add_enabled(false, egui::Button::new("Not available")); }
    }
}

fn card(ui: &mut egui::Ui, i: usize, r: &Row, w: f32, dark: bool, tex: Option<&TextureHandle>, acts: &mut Vec<Act>) {
    let (_, panel, line, _, dim) = palette(dark);
    egui::Frame::new().fill(panel).stroke(Stroke::new(1.0, line)).corner_radius(14).inner_margin(16).show(ui, |ui| {
        ui.set_width(w - 34.0); ui.set_min_height(236.0);
        ui.vertical(|ui| {
        ui.horizontal(|ui| {
            tile(ui, &r.app, tex, 52.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(&r.app.name).size(19.0).strong());
                let (t, c) = status_label(r.status(), &r.app); chip(ui, &t, c);
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(&r.app.tagline).color(dim).size(13.5));
        ui.add_space(4.0);
        let inst = r.found.as_ref().and_then(|f| f.version.clone());
        let line1 = match (&inst, r.latest_tag()) {
            (Some(v), Some(l)) => format!("Installed {v}  ·  latest {l}"),
            (Some(v), None) => format!("Installed {v}"),
            (None, Some(l)) => format!("Latest {l}{}", if let Fetch::Ok(x) = &r.fetch { format!("  ·  {}", date(x)) } else { String::new() }),
            _ => match &r.fetch { Fetch::Loading => "Checking for updates…".to_string(), Fetch::Err(e) => e.clone(), _ => String::new() },
        };
        if !line1.is_empty() { ui.label(RichText::new(line1).size(12.5).color(if matches!(r.fetch, Fetch::Err(_)) && inst.is_none() { Color32::from_rgb(255, 120, 140) } else { dim })); }
        if let Some(j) = &r.job { ui.add(egui::ProgressBar::new(j.frac).text(j.text.clone()).fill(accent(&r.app))); }
        if let Some((bad, n)) = &r.notice { ui.label(RichText::new(n).size(12.5).color(if *bad { Color32::from_rgb(255, 120, 140) } else { Color32::from_rgb(76, 217, 163) })); }
        else if let Some(n) = &r.app.note { if matches!(r.status(), Status::NotPublished | Status::OtherOs | Status::Problem | Status::Android) { ui.label(RichText::new(n).size(12.0).color(dim)); } }
        ui.add_space(6.0);
        ui.horizontal(|ui| buttons(ui, i, r, acts));
        });
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        self.pump();
        let focused = ctx.input(|i| i.focused);
        if focused && !self.was_focused { self.refresh_installed(); }
        self.was_focused = focused;
        let dark = self.cfg.dark;
        let (_, panel, line, _, dim) = palette(dark);
        let mut acts: Vec<Act> = vec![];
        let busy = self.rows.iter().any(|r| r.job.is_some() || matches!(r.fetch, Fetch::Loading));

        egui::TopBottomPanel::top("top").frame(egui::Frame::new().fill(panel).stroke(Stroke::new(1.0, line)).inner_margin(egui::Margin::symmetric(20, 14))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                egui::Image::new(&self.logo).corner_radius(10).paint_at(ui, rect);
                ui.vertical(|ui| { ui.label(RichText::new("Box Launcher").size(21.0).strong()); ui.label(RichText::new("Install, update and open every app in the suite").size(12.5).color(dim)); });
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Settings").clicked() { self.settings = !self.settings; }
                    if ui.button(if dark { "Light" } else { "Dark" }).clicked() { self.cfg.dark = !dark; self.cfg.save(); style(ctx, self.cfg.dark); }
                    if ui.add_enabled(!busy, egui::Button::new("Refresh")).clicked() { self.refresh_installed(); self.refresh_releases(); }
                    ui.add_space(8.0);
                    if ui.selectable_label(self.cfg.table, "Table").clicked() { self.cfg.table = true; self.cfg.save(); }
                    if ui.selectable_label(!self.cfg.table, "Cards").clicked() { self.cfg.table = false; self.cfg.save(); }
                });
            });
        });
        egui::TopBottomPanel::bottom("foot").frame(egui::Frame::new().fill(palette(dark).0).inner_margin(egui::Margin::symmetric(20, 8))).show(ctx, |ui| {
            ui.label(RichText::new(format!("Box Launcher {}  ·  downloads are checked against the release's SHA-256 file  ·  data folder {}", env!("CARGO_PKG_VERSION"), cfg::home().display())).size(11.5).color(dim));
        });

        let table = self.cfg.table;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(palette(dark).0).inner_margin(egui::Margin::symmetric(20, 16))).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if !table {
                    let gap = 14.0;
                    let avail = ui.available_width();
                    let cols = (((avail + gap) / (300.0 + gap)).floor() as usize).max(1);
                    let w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).clamp(290.0, 460.0);
                    ui.spacing_mut().item_spacing = Vec2::new(gap, gap);
                    for (ci, chunk) in self.rows.chunks(cols).enumerate() {
                        ui.horizontal_top(|ui| {
                            for (k, r) in chunk.iter().enumerate() {
                                let tex = r.app.icon.as_ref().and_then(|n| self.tex.get(n));
                                card(ui, ci * cols + k, r, w, dark, tex, &mut acts);
                            }
                        });
                    }
                } else {
                    let mut order: Vec<usize> = (0..self.rows.len()).collect();
                    let sort = self.sort;
                    order.sort_by_key(|&i| { let r = &self.rows[i]; match sort { 1 => format!("{}", r.status() as u8), 2 => r.found.as_ref().and_then(|f| f.version.clone()).unwrap_or_default(), 3 => r.latest_tag().unwrap_or("").to_string(), _ => r.app.name.to_lowercase() } });
                    egui::Grid::new("tbl").num_columns(6).spacing([22.0, 14.0]).striped(true).min_col_width(70.0).show(ui, |ui| {
                        for (k, h) in ["App", "Status", "Installed", "Latest", "Released", ""].iter().enumerate() {
                            if k < 4 { if ui.selectable_label(self.sort == k, RichText::new(*h).strong()).clicked() { self.sort = k; } } else { ui.label(RichText::new(*h).strong()); }
                        }
                        ui.end_row();
                        for i in order {
                            let r = &self.rows[i];
                            ui.horizontal(|ui| { tile(ui, &r.app, r.app.icon.as_ref().and_then(|n| self.tex.get(n)), 28.0); ui.label(RichText::new(&r.app.name).strong()); });
                            let (t, c) = status_label(r.status(), &r.app); chip(ui, &t, c);
                            ui.label(r.found.as_ref().and_then(|f| f.version.clone()).unwrap_or("-".into()));
                            ui.label(r.latest_tag().unwrap_or("-"));
                            ui.label(if let Fetch::Ok(x) = &r.fetch { date(x) } else { "-".into() });
                            ui.horizontal(|ui| { buttons(ui, i, r, &mut acts); if let Some(j) = &r.job { ui.label(&j.text); } });
                            ui.end_row();
                        }
                    });
                    for r in &self.rows { if let Some((bad, n)) = &r.notice { ui.label(RichText::new(format!("{}: {n}", r.app.name)).color(if *bad { Color32::from_rgb(255, 120, 140) } else { Color32::from_rgb(76, 217, 163) })); } }
                }
            });
        });

        if self.settings {
            let mut open = true;
            egui::Window::new("Settings").open(&mut open).collapsible(false).default_width(440.0).show(ctx, |ui| {
                ui.label(RichText::new("GitHub access (for private repositories)").strong());
                ui.label(RichText::new("LinkedBox lives in private repositories. A token that can read them lets the launcher see and download their releases. It is stored only in a file in the data folder that only you can read, and is sent only to api.github.com.").size(12.5).color(dim));
                let mut changed = ui.add(egui::TextEdit::singleline(&mut self.cfg.token).password(true).hint_text("ghp_… or github_pat_…").desired_width(f32::INFINITY)).changed();
                changed |= ui.checkbox(&mut self.cfg.use_gh, "Also use my GitHub CLI login (gh auth token)").changed();
                ui.label(RichText::new("The GITHUB_TOKEN environment variable is used too, if set.").size(12.0).color(dim));
                if changed { self.cfg.save(); }
                if ui.button("Check releases again").clicked() { self.refresh_releases(); }
                ui.separator();
                ui.label(RichText::new("The list of apps").strong());
                ui.label(RichText::new(format!("Built in. Put a catalog.json in {} to replace it (same format as the built-in one).", cfg::home().display())).size(12.5).color(dim));
            });
            self.settings = open;
        }

        for a in acts { self.act(a); }

        // screenshot mode for testing: wait until the checks answered, save one frame and quit
        if let Some((path, t0)) = self.shot.clone() {
            let ready = !self.rows.iter().any(|r| matches!(r.fetch, Fetch::Loading));
            if (ready && t0.elapsed().as_millis() > 1200) || t0.elapsed().as_secs() > 20 {
                if !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Screenshot { .. }))) { ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default())); }
            }
            let img = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            if let Some(im) = img {
                let buf = image::RgbaImage::from_raw(im.width() as u32, im.height() as u32, im.as_raw().to_vec()).unwrap();
                let _ = buf.save(&path); std::process::exit(0);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if busy { ctx.request_repaint_after(std::time::Duration::from_millis(120)); }
    }
}

/// `boxlauncher --status`: the same checks without a window
fn status_cli() {
    let cfg = Config::load();
    let st = State::load();
    let token = cfg.github_token();
    for app in catalog::load() {
        let found = install::find(&app, &st);
        let os = if app.is_android() { "android" } else { catalog::os() };
        let rel = app.repo.as_ref().map(|r| github::latest(r, token.as_deref()));
        let latest = match &rel { Some(Ok(r)) => format!("latest {} ({})", r.tag_name, github::pick_asset(r, os).map(|a| a.name.clone()).unwrap_or_else(|| format!("no {os} installer"))), Some(Err(e)) => format!("cannot check: {e}"), None => "no releases (local only)".into() };
        println!("{:<20} {:<12} {}  |  {}", app.name, if app.is_android() { "phone app".to_string() } else if !app.runs_here() { "other OS".to_string() } else { "this OS".to_string() }, found.map(|f| format!("installed {} at {}", f.version.unwrap_or("?".into()), f.path.display())).unwrap_or("not installed".into()), latest);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--status") { status_cli(); return Ok(()); }
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1).cloned());
    let im = image::load_from_memory(ICON_LAUNCHER).expect("icon").to_rgba8();
    let icon = egui::IconData { width: im.width(), height: im.height(), rgba: im.into_raw() };
    let opts = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1060.0, 700.0]).with_min_inner_size([620.0, 420.0]).with_title("Box Launcher").with_icon(icon), ..Default::default() };
    eframe::run_native("Box Launcher", opts, Box::new(move |cc| Ok(Box::new(App::new(cc, shot.clone())))))
}
