fn main() {
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/app.ico");
    winresource::WindowsResource::new()
        .set_icon("assets/app.ico")
        .set_manifest_file("assets/app.manifest")
        .set("ProductName", "World Clock")
        .set("FileDescription", "World Clock — Desktop & Tray")
        .set("LegalCopyright", "MIT License")
        .compile()
        .expect("Could not compile Windows resources");
}
