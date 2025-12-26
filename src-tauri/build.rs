fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    {
        // Ensure Swift runtime is discoverable for ScreenCaptureKit at runtime.
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx");
    }
}
