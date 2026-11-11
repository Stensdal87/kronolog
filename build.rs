// Lægger ikon og versionsoplysninger ind i .exe-filen, når der bygges på Windows.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if cfg!(windows) && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "KronoLog");
        res.set("FileDescription", "KronoLog");
        res.compile().expect("kunne ikke indlejre Windows-ressourcer");
    }
}
