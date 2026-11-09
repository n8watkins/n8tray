//! Discovery: every `*.json` in the modules folder becomes an app (spec 3,
//! plan 4.1). A manifest that cannot be used is reported, never dropped
//! silently: the hub shows it as a problem row.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use n8tray_contract::manifest::{Manifest, StatusSource};
use n8tray_contract::validate::{Kind, validate_as};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;

/// A manifest the host can use.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovered {
    pub manifest: Manifest,
    pub file: PathBuf,
    /// Things worth knowing that do not stop the app, such as fields this
    /// host does not know (spec section 10: ignored at run time).
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    /// Not a valid manifest.
    Invalid,
    /// A contract major newer than this host's.
    NewerContract,
    /// A contract 1 `modular.json`: the PowerShell tray's format.
    ContractOne,
    /// A `builtin` manifest in the modules folder, which only the host ships.
    Builtin,
    /// Another manifest with the same id won.
    Duplicate,
}

/// A manifest the host cannot use, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub file: PathBuf,
    pub id: Option<String>,
    pub kind: ProblemKind,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discovery {
    pub apps: BTreeMap<String, Discovered>,
    pub problems: Vec<Problem>,
}

/// Reads every manifest in `dir`. A missing folder has no apps.
pub fn discover(dir: &Path) -> Discovery {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                    && !p
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();

    let mut discovery = Discovery::default();
    for file in files {
        match read_manifest(&file) {
            Err(problem) => discovery.problems.push(problem),
            Ok(found) => {
                let id = found.manifest.id.clone();
                match discovery.apps.remove(&id) {
                    None => {
                        discovery.apps.insert(id, found);
                    }
                    Some(existing) => {
                        let (winner, loser) = if newer(&found.manifest, &existing.manifest) {
                            (found, existing)
                        } else {
                            (existing, found)
                        };
                        discovery.problems.push(Problem {
                            file: loser.file.clone(),
                            id: Some(id.clone()),
                            kind: ProblemKind::Duplicate,
                            message: format!(
                                "Another manifest for {id} wins: {} (version {})",
                                winner.file.display(),
                                winner.manifest.version
                            ),
                        });
                        discovery.apps.insert(id, winner);
                    }
                }
            }
        }
    }
    discovery
}

/// Whether `a` is a newer version than `b` (SemVer; unparsable versions lose).
fn newer(a: &Manifest, b: &Manifest) -> bool {
    match (
        semver::Version::parse(&a.version),
        semver::Version::parse(&b.version),
    ) {
        (Ok(a), Ok(b)) => a > b,
        (Ok(_), Err(_)) => true,
        _ => false,
    }
}

/// Whether a schema issue is only an unknown field, which a host ignores at
/// run time (spec section 10).
fn is_unknown_field(issue: &n8tray_contract::Issue) -> bool {
    issue.code == "schema"
        && issue
            .message
            .contains("Additional properties are not allowed")
}

pub fn read_manifest(file: &Path) -> Result<Discovered, Problem> {
    let problem = |kind, id: Option<String>, message: String| Problem {
        file: file.to_path_buf(),
        id,
        kind,
        message,
    };
    let text = std::fs::read_to_string(file).map_err(|e| {
        problem(
            ProblemKind::Invalid,
            None,
            format!("Manifest unreadable: {e}"),
        )
    })?;
    let value = n8tray_contract::parse_json(&text).map_err(|e| {
        problem(
            ProblemKind::Invalid,
            None,
            format!("Manifest invalid: not JSON ({e})"),
        )
    })?;
    let id = value.get("id").and_then(Value::as_str).map(str::to_string);
    let contract = value.get("contract").and_then(Value::as_u64);
    if value.get("schemaVersion").is_some() || contract == Some(1) {
        return Err(problem(
            ProblemKind::ContractOne,
            id,
            "A contract 1 modular.json: the PowerShell N8 Tray reads it; this one needs a contract 2 manifest".into(),
        ));
    }
    if let Some(major) = contract.filter(|c| *c > u64::from(n8tray_contract::CONTRACT)) {
        return Err(problem(
            ProblemKind::NewerContract,
            id,
            format!("Needs a newer N8 Tray (contract {major})"),
        ));
    }
    let file_name = file.file_name().and_then(|n| n.to_str());
    let report = validate_as(Kind::Manifest, &value, file_name);
    let (ignored, issues): (Vec<_>, Vec<_>) = report.issues.into_iter().partition(is_unknown_field);
    if let Some(first) = issues.first() {
        return Err(problem(
            ProblemKind::Invalid,
            id,
            format!("Manifest invalid: {first}"),
        ));
    }
    let manifest: Manifest = serde_json::from_value(value).map_err(|e| {
        problem(
            ProblemKind::Invalid,
            id.clone(),
            format!("Manifest invalid: {e}"),
        )
    })?;
    if matches!(manifest.status, StatusSource::Builtin) {
        return Err(problem(
            ProblemKind::Builtin,
            id,
            "A builtin manifest in the modules folder: only N8 Tray ships those".into(),
        ));
    }
    let notes = ignored
        .iter()
        .map(|i| format!("ignored: {}", i.message))
        .collect();
    Ok(Discovered {
        manifest,
        file: file.to_path_buf(),
        notes,
    })
}

/// Watches the modules folder and sends `()` on every change. Keep the
/// watcher alive as long as changes matter.
pub fn watch(dir: &Path, changed: Sender<()>) -> notify::Result<RecommendedWatcher> {
    std::fs::create_dir_all(dir).map_err(notify::Error::io)?;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = changed.send(());
        }
    })?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, version: &str) -> String {
        format!(
            r#"{{ "contract": 2, "id": "{id}", "name": "App {id}", "version": "{version}",
                 "installDir": "/opt/{id}", "status": {{ "type": "file" }} }}"#
        )
    }

    #[test]
    fn reads_valid_manifests_and_reports_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        write("alpha.json", &manifest("alpha", "1.0.0"));
        write("broken.json", "{ not json");
        write("wrong-name.json", &manifest("other", "1.0.0"));
        write(
            "newer.json",
            r#"{ "contract": 3, "id": "newer", "name": "N", "version": "1.0.0" }"#,
        );
        write(
            "old.json",
            r#"{ "schemaVersion": 2, "id": "old", "name": "Old" }"#,
        );
        write(
            "wslmemory.json",
            r#"{ "contract": 2, "id": "wslmemory", "name": "WSL Memory", "version": "1.0.0", "status": { "type": "builtin" } }"#,
        );
        write("notes.txt", "not a manifest");
        write(".hidden.json", "{}");

        let found = discover(dir.path());
        assert_eq!(found.apps.keys().collect::<Vec<_>>(), ["alpha"]);
        let kinds: Vec<_> = found.problems.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            [
                ProblemKind::Invalid,
                ProblemKind::NewerContract,
                ProblemKind::ContractOne,
                ProblemKind::Invalid,
                ProblemKind::Builtin,
            ]
        );
        assert!(found.problems[3].message.contains("id-file-name"));
        assert_eq!(
            found.problems[1].message,
            "Needs a newer N8 Tray (contract 3)"
        );
    }

    #[test]
    fn unknown_fields_are_ignored_with_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let text = manifest("alpha", "1.0.0")
            .replace(r#""contract": 2,"#, r#""contract": 2, "laterField": true,"#);
        std::fs::write(dir.path().join("alpha.json"), text).unwrap();
        let found = discover(dir.path());
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(found.apps["alpha"].notes.len(), 1);
    }

    #[test]
    fn the_newer_of_two_manifests_wins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("alpha.json"), manifest("alpha", "1.2.0")).unwrap();
        // The same id under a name that the file-name rule allows.
        std::fs::write(
            dir.path().join("manifest.json"),
            manifest("alpha", "1.10.0"),
        )
        .unwrap();
        let found = discover(dir.path());
        assert_eq!(found.apps["alpha"].manifest.version, "1.10.0");
        assert_eq!(found.problems.len(), 1);
        assert_eq!(found.problems[0].kind, ProblemKind::Duplicate);
    }

    #[test]
    fn a_missing_folder_has_no_apps() {
        let found = discover(Path::new("/does/not/exist/n8tray"));
        assert!(found.apps.is_empty() && found.problems.is_empty());
    }
}
