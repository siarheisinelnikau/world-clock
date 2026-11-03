fn main() {
    println!("cargo:rerun-if-changed=native/taskbar_reservation.cpp");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let compiler = cc::Build::new().cpp(true).get_compiler();
    let result = compiler
        .to_command()
        .args([
            "/nologo",
            "/std:c++20",
            "/EHsc",
            "/O2",
            "/MT",
            "/LD",
            "/DWIN32_LEAN_AND_MEAN",
            "/DNOMINMAX",
        ])
        .arg("native/taskbar_reservation.cpp")
        .arg(format!(
            "/Fo{}",
            output.join("taskbar_reservation.obj").display()
        ))
        .args([
            "/link",
            "/EXPORT:DllGetClassObject",
            "/EXPORT:DllCanUnloadNow",
            "windowsapp.lib",
            "ole32.lib",
            "oleaut32.lib",
            "user32.lib",
        ])
        .arg(format!(
            "/OUT:{}",
            output.join("WorldClock.Taskbar.dll").display()
        ))
        .arg(format!(
            "/IMPLIB:{}",
            output.join("WorldClock.Taskbar.lib").display()
        ))
        .output()
        .expect("Could not run the C++ compiler for the embedded taskbar module");
    if !result.status.success() {
        panic!(
            "Could not build the embedded taskbar module:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
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
