#[cfg(target_os = "macos")]
fn main() {
    // `screencapturekit` links against Swift runtime libraries. When building a plain
    // CLI binary, `@rpath` may be missing, causing dyld to fail to locate Swift dylibs.
    //
    // For macOS Sequoia (15+) the Swift runtime is available on the system, but the
    // loader still needs an rpath to find it.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    // Development fallback for systems that only have Swift runtime via CLT/Xcode.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx");
}

#[cfg(not(target_os = "macos"))]
fn main() {}

