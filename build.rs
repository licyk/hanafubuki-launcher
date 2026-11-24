fn main() {
    println!("cargo:rerun-if-changed=windows/launcher.rc");
    println!("cargo:rerun-if-changed=windows/launcher.manifest");
    println!("cargo:rerun-if-changed=windows/launcher.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("windows/launcher.rc", embed_resource::NONE)
            .manifest_required()
            .expect("compile launcher Windows resources");
    }
}
