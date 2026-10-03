fn main() {
    windows_reactor_setup::as_self_contained();
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/clipboard.ico");
    embed_resource::compile("assets/app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("Could not embed the application icon");
}
