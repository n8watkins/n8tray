fn main() {
    // The window may call exactly these commands, and only the window the
    // capability in capabilities/hub.json names (plan 9.2).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_snapshot",
            "module_action",
            "open_item",
            "hide_window",
            "app_icon",
            "set_setting",
            "host_command",
            "open_host",
            "open_update",
            "history",
            "recent_warnings",
            "status_text",
            "fit_window",
            "initial_view",
        ]),
    ))
    .expect("the Tauri build step runs");
}
