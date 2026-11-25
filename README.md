# Box Launcher

One place to install, update and open the apps of the suite: PaperBox, LinkedBox (agent and phone app), DriveBox and NodeBox.
A small native app (Rust + egui, no web view), like the ArtCraft Launcher it is modelled on.

- **Cards or table** of every app with its status, installed version, latest release and date.
- **Install / Update** from the app's GitHub releases: picks the right file for the OS (`.dmg` on macOS, portable `.zip` first, then `.exe` / `.msi` on Windows (the zip needs no administrator rights and works on PCs that block installers), `.apk` for the phone app),
  downloads it and checks it against the release's `SHA256SUMS.txt` before installing. A file that does not match is deleted and nothing is installed.
- **Open** an installed app, or show it in Finder / Explorer.
- Apps without a public release for this computer yet (PaperBox, and DriveBox on a Mac) are listed and opened from the copy installed here. DriveBox runs on Windows and macOS.
- **Private apps** (LinkedBox) are shown locked (greyed, with a lock icon) until you enter a **license code** under Settings (it is a GitHub access token underneath). It is kept in `~/.boxlauncher/config.json` (readable only by you) and sent only to `api.github.com`.
  The `GITHUB_TOKEN` variable and the GitHub CLI login (opt-in) also work.
- Dark / light theme in the suite colours.

## The app list

Built in (`catalog.json`). Put your own `~/.boxlauncher/catalog.json` to replace it: id, name, tagline, accent colour, `repo` (`owner/name`), `platforms`, `mac_app`, `win_exe`.

## Build

    cargo run                      # window
    cargo run -- --status          # the same checks without a window
    cargo test                     # unit tests; add -- --ignored for a real download check
    scripts/bundle-mac.sh          # dist/BoxLauncher.app

Needs Rust 1.85+. Windows: `cargo build --release` gives `boxlauncher.exe` (icon embedded). The workflow `.github/workflows/windows.yml` (run it by hand from the Actions tab, or push a `v*` tag) builds `BoxLauncher-<version>-x64.msi` with WiX (`packaging/boxlauncher.wxs`) plus a portable zip and a SHA-256 file.

## Not done yet

Launcher self-update, tray icon, languages, Linux packaging, a Windows installer, and installing on Windows has not been tried on a real Windows PC.

MIT licence.

The lock icon is from [Lucide](https://lucide.dev) (ISC licence), drawn in code.
