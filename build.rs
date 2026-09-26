//! Windows: embed the app icon into kerf.exe. Other platforms: nothing to do.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/kerf.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/kerf.ico");
        res.set("ProductName", "Kerf");
        res.set("FileDescription", "Kerf — fast diff tool");
        res.compile().expect("embed Windows resources");
    }
}
