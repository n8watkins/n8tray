//! Every fixture and example in `spec/` against the schemas, the rules of
//! spec section 11 and the Rust types.
//!
//! - `spec/fixtures/<kind>/valid/*.json` must be valid and read back the same
//!   through the types.
//! - `spec/fixtures/<kind>/invalid/<code>.<name>.json` must be invalid with
//!   that code.
//! - `spec/examples/*.json` are manifests named after their id.
//! - Every code in the spec's table has an invalid fixture, and every code a
//!   fixture expects is in the table.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use n8tray_contract::{
    Kind, lease::Lease, manifest::Manifest, parse_json, status::Status, validate_as,
};
use serde_json::Value;

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} has no fixtures", dir.display());
    files
}

fn read(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap();
    parse_json(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Numbers compared as f64, so `12` and `12.0` are the same value.
fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| k.as_str() != "$schema")
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The document reads through the types and writes back to the same JSON,
/// so the types cover every field the schema allows.
fn round_trips(kind: Kind, value: &Value, path: &Path) {
    let written = match kind {
        Kind::Manifest => {
            serde_json::to_value(serde_json::from_value::<Manifest>(value.clone()).unwrap())
        }
        Kind::Status => {
            serde_json::to_value(serde_json::from_value::<Status>(value.clone()).unwrap())
        }
        Kind::Lease => {
            serde_json::to_value(serde_json::from_value::<Lease>(value.clone()).unwrap())
        }
        Kind::ActionResult => serde_json::to_value(
            serde_json::from_value::<n8tray_contract::action::ActionResult>(value.clone()).unwrap(),
        ),
        Kind::ModularV1 => return,
    }
    .unwrap();
    assert_eq!(
        normalize(&normalize_times(&written)),
        normalize(&normalize_times(value)),
        "{} does not read back the same through the types",
        path.display()
    );
}

/// The time fields compare as instants, not text: `.000Z` and `Z` are equal.
fn normalize_times(value: &Value) -> Value {
    match value {
        Value::String(s) if s.len() >= 20 && s.ends_with('Z') && s.as_bytes()[10] == b'T' => {
            match chrono::DateTime::parse_from_rfc3339(s) {
                Ok(t) => Value::String(t.timestamp_millis().to_string()),
                Err(_) => value.clone(),
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize_times).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalize_times(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn valid_fixtures_are_valid_and_read_back_the_same() {
    for kind in Kind::ALL {
        for path in json_files(&spec_dir().join("fixtures").join(kind.name()).join("valid")) {
            let value = read(&path);
            let report = validate_as(kind, &value, None);
            assert!(
                report.is_valid(),
                "{} should be valid: {:#?}",
                path.display(),
                report.issues
            );
            round_trips(kind, &value, &path);
        }
    }
}

#[test]
fn invalid_fixtures_fail_with_their_code() {
    for kind in Kind::ALL {
        for path in json_files(
            &spec_dir()
                .join("fixtures")
                .join(kind.name())
                .join("invalid"),
        ) {
            let name = path.file_name().unwrap().to_str().unwrap();
            let expected = name.split('.').next().unwrap();
            let report = validate_as(kind, &read(&path), None);
            assert!(!report.is_valid(), "{} should be invalid", path.display());
            assert!(
                report.codes().contains(&expected),
                "{} should fail with {expected}, got {:#?}",
                path.display(),
                report.issues
            );
        }
    }
}

#[test]
fn examples_are_valid_manifests_named_after_their_id() {
    let examples = json_files(&spec_dir().join("examples"));
    let names: BTreeSet<_> = examples
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    for expected in [
        "voicedock.json",
        "n8commentary.json",
        "n8hotkeys.json",
        "leadscraper.json",
        "wslmemory.json",
    ] {
        assert!(
            names.contains(expected),
            "spec/examples/{expected} is missing"
        );
    }
    for path in examples {
        let value = read(&path);
        let name = path.file_name().unwrap().to_str().unwrap();
        let report = n8tray_contract::validate(&value, Some(name));
        assert_eq!(
            report.kind,
            Kind::Manifest,
            "{name} is not detected as a manifest"
        );
        assert!(
            report.is_valid(),
            "{name} should be valid: {:#?}",
            report.issues
        );
        round_trips(Kind::Manifest, &value, &path);
    }
}

/// The codes in the table of spec section 11.
fn spec_codes() -> BTreeSet<String> {
    let spec = fs::read_to_string(spec_dir().join("contract-v2.md")).unwrap();
    let section = spec
        .split("## 11. ")
        .nth(1)
        .expect("the spec has section 11");
    section
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split('`').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_rule_in_the_spec_has_a_fixture_and_every_fixture_code_is_in_the_spec() {
    let documented = spec_codes();
    assert!(
        documented.len() >= 15,
        "spec section 11 lists only {documented:?}"
    );
    let mut covered = BTreeSet::new();
    for kind in Kind::ALL {
        for path in json_files(
            &spec_dir()
                .join("fixtures")
                .join(kind.name())
                .join("invalid"),
        ) {
            let code = path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .split('.')
                .next()
                .unwrap()
                .to_owned();
            assert!(
                code == "schema" || documented.contains(&code),
                "{} expects code {code}, which spec section 11 does not document",
                path.display()
            );
            covered.insert(code);
        }
    }
    // id-file-name depends on the file's name, so it is tested in the crate's unit tests instead.
    for code in documented.iter().filter(|c| c.as_str() != "id-file-name") {
        assert!(
            covered.contains(code),
            "spec section 11's {code} has no invalid fixture"
        );
    }
}

/// Every example app has the icon spec section 3.11 describes, drawn as an
/// image only: a 32 x 32 SVG with no scripts, links or external references.
#[test]
fn examples_have_icons_that_follow_the_rules() {
    for path in json_files(&spec_dir().join("examples")) {
        let id = path.file_stem().unwrap().to_str().unwrap();
        let icon = spec_dir().join("examples/icons").join(format!("{id}.svg"));
        let svg = fs::read_to_string(&icon)
            .unwrap_or_else(|e| panic!("spec/examples/icons/{id}.svg: {e}"));
        assert!(svg.len() <= 256 * 1024, "{id}.svg is over 256 KB");
        assert!(
            svg.trim_start().starts_with("<svg") && svg.contains(r#"viewBox="0 0 32 32""#),
            "{id}.svg should be one <svg> element with a 32 x 32 viewBox"
        );
        let lower = svg.to_lowercase();
        for banned in ["<script", "href", "url(http", "<foreignobject", " on"] {
            let found = if banned == " on" {
                lower
                    .split_whitespace()
                    .any(|w| w.starts_with("on") && w.contains('='))
            } else {
                lower.contains(banned)
            };
            assert!(!found, "{id}.svg contains {banned:?}");
        }
    }
}
