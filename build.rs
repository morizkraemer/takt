fn main() {
    println!("cargo:rerun-if-changed=assets/takt.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/takt.ico");
            res.compile().expect("failed to compile Windows resources");
        }
    }
}
