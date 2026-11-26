fn main() {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=assets");
    slint_build::compile("ui/window.slint").expect("ui/window.slint compiles");
    embed_windows_resources();
}

/// The exe's icon, version details (product N8 Tray, company n8builds.dev)
/// and manifest (per-monitor DPI aware, Windows 10 and 11, as invoker).
///
/// Only when the build host is Windows: the resource compiler is `rc.exe`.
#[cfg(windows)]
fn embed_windows_resources() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let version = root_version(&manifest_dir.join("../Cargo.toml"));
    let mut numbers = version
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (
        numbers.next().unwrap_or(0),
        numbers.next().unwrap_or(0),
        numbers.next().unwrap_or(0),
    );
    let path = |p: PathBuf| p.to_string_lossy().replace('\\', "/");
    // The app icon is the Tauri app's (D30) until the native shell replaces it.
    let icon = path(manifest_dir.join("../app/src-tauri/icons/icon.ico"));
    let manifest = path(manifest_dir.join("assets/n8tray-native.manifest"));
    println!("cargo:rerun-if-changed=../app/src-tauri/icons/icon.ico");

    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"
1 24 "{manifest}"
VS_VERSION_INFO VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
FILESUBTYPE VFT2_UNKNOWN
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "n8builds.dev"
      VALUE "FileDescription", "N8 Tray"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "n8tray-native"
      VALUE "LegalCopyright", "Copyright (c) 2026 n8builds.dev"
      VALUE "OriginalFilename", "n8tray-native.exe"
      VALUE "ProductName", "N8 Tray"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_path = out_dir.join("n8tray-native.rc");
    std::fs::write(&rc_path, rc).expect("n8tray-native.rc is written");
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .expect("the resources compile");
}

#[cfg(not(windows))]
fn embed_windows_resources() {}

/// N8 Tray's version: `[workspace.package] version` in the root Cargo.toml,
/// which release-kit keeps (this crate is a workspace of its own, D33).
#[cfg(windows)]
fn root_version(path: &std::path::Path) -> String {
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(path).expect("the root Cargo.toml is readable");
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(value) = line.strip_prefix("version") {
            let value = value.trim_start().trim_start_matches('=').trim();
            return value.trim_matches('"').to_string();
        }
    }
    panic!("no [workspace.package] version in {}", path.display());
}
