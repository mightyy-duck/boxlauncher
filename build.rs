// Windows only: put the icon into boxlauncher.exe
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Box Launcher");
        res.set("FileDescription", "Box Launcher");
        res.compile().expect("embed the Windows icon");
    }
}
