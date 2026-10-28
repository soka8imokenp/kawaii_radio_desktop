// Windows: the exe carries the app's icon (Explorer, the taskbar, Alt+Tab)
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("../app/icons/icon.ico");
        r.set("ProductName", "Kawaii Radio");
        r.set("FileDescription", "Kawaii Radio");
        if let Err(e) = r.compile() {
            println!("cargo:warning=icon: {e}");
        }
    }
}
