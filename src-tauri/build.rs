fn main() {
    // Cargo does not otherwise rerun tauri-build when bundle icon files change.
    // Track both the Windows resource icon and the PNG used by other targets
    // so `tauri dev` embeds a freshly replaced app icon on the next build.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    tauri_build::build()
}
