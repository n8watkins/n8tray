//! The snapshot fixtures in `native/fixtures/` (the native window's tests and
//! golden images read them) must keep matching the model: each file loads into
//! a `HostSnapshot` and serializes back to exactly the same JSON, so a renamed,
//! added or removed field fails here instead of in a picture.

use std::path::PathBuf;

use n8tray_core::model::HostSnapshot;
use n8tray_core::pins;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../native/fixtures")
}

/// JSON written by JavaScript says `0` where serde writes `0.0` for an `f64`;
/// they are the same number, so every number is compared as an `f64`.
fn normalized(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(n) => serde_json::Number::from_f64(n.as_f64().expect("finite"))
            .map_or(Value::Null, Value::Number),
        Value::Array(items) => Value::Array(items.into_iter().map(normalized).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, normalized(v))).collect())
        }
        other => other,
    }
}

/// The pin figures are what the host would write for the fixture's apps
/// (D32), so the native window's pictures show reachable states.
fn pins_agree(name: &str, s: &HostSnapshot) {
    for app in &s.apps {
        assert_eq!(
            app.in_main_view,
            pins::in_main_view(app, false),
            "{name}: {} inMainView",
            app.id
        );
    }
    let hub: Vec<_> = s.apps.iter().filter(|a| pins::in_hub(a)).collect();
    let counts = pins::counts(&hub);
    assert_eq!(s.more, counts.more, "{name}: more");
    assert_eq!(
        (s.pinned_running, s.pinned_total, s.pinned_text.as_str()),
        (
            counts.pinned_running,
            counts.pinned_total,
            counts.pinned_text.as_str()
        ),
        "{name}: pinned figures"
    );
}

/// The tooltip is what the host would write for the fixture: its headline,
/// the running apps in the main view, then the notes with N8's memory last
/// (D34), so the native tray's pictures show real hover text.
fn tooltip_agrees(name: &str, s: &HostSnapshot) {
    let hub: Vec<_> = s.apps.iter().filter(|a| pins::in_hub(a)).collect();
    let counts = pins::counts(&hub);
    let mut notes = Vec::new();
    if s.legacy_tray {
        notes.push(n8tray_core::host::WATCHING_NOTE.to_string());
    }
    notes.push(s.memory.text.clone());
    assert_eq!(
        s.memory.text,
        format!("N8 {} in all", n8tray_core::util::mb_text(s.memory.n8_mb)),
        "{name}: memory.text"
    );
    let expected = n8tray_core::tooltip::format(
        &format!("N8 Tray: {}", s.headline),
        &counts.shown_running,
        counts.more.as_ref().map_or(0, |m| m.running as usize),
        &notes,
    );
    assert_eq!(s.tooltip, expected, "{name}: tooltip");
}

#[test]
fn every_fixture_loads_and_round_trips() {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("native/fixtures exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&path).expect("readable fixture");
        let original: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: not JSON: {e}"));
        let snapshot: HostSnapshot = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{name} no longer matches HostSnapshot: {e}"));
        let again = serde_json::to_value(&snapshot).expect("serializes");
        assert_eq!(
            normalized(original),
            normalized(again.clone()),
            "{name}: serializing the loaded snapshot changes the JSON"
        );
        let reloaded: HostSnapshot = serde_json::from_value(again).expect("deserializes again");
        assert_eq!(snapshot, reloaded, "{name}: second round trip differs");
        pins_agree(&name, &snapshot);
        tooltip_agrees(&name, &snapshot);
        names.push(name);
    }
    names.sort();
    assert_eq!(
        names,
        [
            "busy.json",
            "empty.json",
            "healthy.json",
            "pinned.json",
            "trouble.json",
            "updateFailed.json",
            "updating.json",
            "watching.json"
        ],
        "the fixture set changed; update this list and the interface fixtures together"
    );
}
