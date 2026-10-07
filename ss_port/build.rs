//! Windows builds: the application icon (assets/icon/icon.ico) as an exe
//! resource, so Explorer and the taskbar show it.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon/icon.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/icon/icon.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
