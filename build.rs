//! Places the vendored ConPTY beside the built exe: `conpty.dll` and `x64\OpenConsole.exe`.
//! The inbox Windows ConPTY swallows OSC 10/11/4 queries; this one passes them.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=vendor/conpty");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    // OUT_DIR is target\<profile>\build\ankar-<hash>\out (newer cargo: build\ankar\<hash>\out);
    // the exe lives in target\<profile>.
    let Some(profile_dir) = Path::new(&out_dir)
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "build"))
        .and_then(Path::parent)
    else {
        return;
    };
    let vendor = Path::new("vendor/conpty");
    copy(&vendor.join("conpty.dll"), &profile_dir.join("conpty.dll"));
    copy(&vendor.join("x64/OpenConsole.exe"), &profile_dir.join("x64/OpenConsole.exe"));
}

fn copy(from: &Path, to: &Path) {
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // A running ankar.exe keeps the dll locked; the copy already there is the same file.
    if let Err(err) = std::fs::copy(from, to) {
        println!("cargo:warning=could not copy {} to {}: {err}", from.display(), to.display());
    }
}
