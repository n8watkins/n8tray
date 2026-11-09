//! The host's settings: one JSON file, the single source of truth for what
//! the user chose. Written atomically as UTF-8 without a BOM; a file that does
//! not parse is reported and never overwritten (the PowerShell tray's rules).
//!
//! The PowerShell tray's `config\settings.json` (schema 5 and earlier) is
//! imported once, after a backup, by [`import_powershell`].

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::util::write_atomic;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub startup: Startup,
    pub memory: MemorySettings,
    /// Notifications for problems (red) only, once per episode.
    pub notifications: bool,
    pub updates: UpdateSettings,
    pub history: HistorySettings,
    /// Per app, by id. An app missing here has the defaults.
    pub apps: BTreeMap<String, AppSettings>,
    /// Sections this version does not know (kept as they are, such as the
    /// imported `wslMemory` section the built-in will read).
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            startup: Startup::default(),
            memory: MemorySettings::default(),
            notifications: true,
            updates: UpdateSettings::default(),
            history: HistorySettings::default(),
            apps: BTreeMap::new(),
            other: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Startup {
    /// Start N8 Tray at sign-in. Off until the user turns it on.
    pub launch_at_sign_in: bool,
}

/// System memory levels: attention below `attentionBelowGiB` free, problem
/// below `problemBelowGiB` (the PowerShell tray's 3 and 1.5 GB).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MemorySettings {
    #[serde(rename = "attentionBelowGiB")]
    pub attention_below_gib: f64,
    #[serde(rename = "problemBelowGiB")]
    pub problem_below_gib: f64,
}

impl Default for MemorySettings {
    fn default() -> Self {
        MemorySettings {
            attention_below_gib: 3.0,
            problem_below_gib: 1.5,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdateSettings {
    pub channel: Channel,
    /// Look for updates at start and every 24 h. Nothing installs without a click.
    pub check: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        UpdateSettings {
            channel: Channel::Stable,
            check: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    /// Days of per-minute history kept on disk.
    pub keep_days: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        HistorySettings { keep_days: 7 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Turned off: never started, not hosted, shown as off.
    pub enabled: bool,
    /// Start with N8 Tray. `None` follows the manifest's `autoStartDefault`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
    /// "Use its own icon": the app is not hosted and not in the hub.
    pub own_icon: bool,
    /// The user's alert thresholds, over the manifest's budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetOverride>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            enabled: true,
            auto_start: None,
            own_icon: false,
            budget: None,
            other: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warn_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_warn_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sustain_seconds: Option<u32>,
}

impl Settings {
    /// The app's settings, or the defaults.
    pub fn app(&self, id: &str) -> AppSettings {
        self.apps.get(id).cloned().unwrap_or_default()
    }

    pub fn app_mut(&mut self, id: &str) -> &mut AppSettings {
        self.apps.entry(id.to_string()).or_default()
    }
}

/// The settings as loaded, with the reason they could not be read.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    /// The file did not parse: the defaults are in use and nothing is saved
    /// until the file is fixed and reloaded.
    pub error: Option<String>,
    pub path: PathBuf,
}

impl Loaded {
    /// Saves the settings, unless the file had a parse error.
    pub fn save(&self) -> io::Result<()> {
        if let Some(error) = &self.error {
            return Err(io::Error::other(format!(
                "not saved: {} has an error ({error}); fix it and reload",
                self.path.display()
            )));
        }
        save(&self.path, &self.settings)
    }
}

/// Reads the settings file; a missing file is the defaults.
pub fn load(path: &Path) -> Loaded {
    let (settings, error) = match std::fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Settings::default(), None),
        Err(e) => (Settings::default(), Some(e.to_string())),
        Ok(text) => match n8tray_contract::parse_json(&text)
            .map_err(|e| e.to_string())
            .and_then(|v| serde_json::from_value::<Settings>(v).map_err(|e| e.to_string()))
        {
            Ok(settings) if settings.schema_version > SCHEMA_VERSION => (
                Settings::default(),
                Some(format!(
                    "schemaVersion {} is newer than this N8 Tray reads ({SCHEMA_VERSION})",
                    settings.schema_version
                )),
            ),
            Ok(settings) => (settings, None),
            Err(e) => (Settings::default(), Some(e)),
        },
    };
    Loaded {
        settings,
        error,
        path: path.to_path_buf(),
    }
}

pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    text.push('\n');
    write_atomic(path, text.as_bytes())
}

/// What an import did, for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub settings: Settings,
    pub notes: Vec<String>,
}

/// The PowerShell tray's module ids that changed with contract 2.
fn contract2_id(powershell_id: &str) -> &str {
    match powershell_id {
        "n8voice" => "voicedock",
        other => other,
    }
}

/// Converts the PowerShell tray's settings (schema 5 and earlier) into these
/// settings, on top of `base`. Unknown sections are kept under their names.
pub fn import_powershell(text: &str, base: Settings) -> Result<Imported, String> {
    let value = n8tray_contract::parse_json(text).map_err(|e| e.to_string())?;
    let object = value
        .as_object()
        .ok_or("the settings file is not a JSON object")?;
    let schema = object
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if schema > 5 {
        return Err(format!(
            "schemaVersion {schema} is newer than the import knows (5)"
        ));
    }
    let mut settings = base;
    let mut notes = vec![format!(
        "imported PowerShell tray settings, schema {schema}"
    )];

    if let Some(on) = object
        .get("startup")
        .and_then(|s| s.get("launchOnWindowsStartup"))
        .and_then(Value::as_bool)
    {
        settings.startup.launch_at_sign_in = on;
    }
    let memory = object.get("memory");
    if let Some(v) = memory
        .and_then(|m| m.get("windowsAmberBelowGiB"))
        .and_then(Value::as_f64)
    {
        settings.memory.attention_below_gib = v;
    }
    if let Some(v) = memory
        .and_then(|m| m.get("windowsRedBelowGiB"))
        .and_then(Value::as_f64)
    {
        settings.memory.problem_below_gib = v;
    }
    for module in object
        .get("modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = module.get("id").and_then(Value::as_str) else {
            continue;
        };
        let new_id = contract2_id(id);
        if new_id != id {
            notes.push(format!("module {id} is now {new_id}"));
        }
        let app = settings.app_mut(new_id);
        if let Some(enabled) = module.get("enabled").and_then(Value::as_bool) {
            app.enabled = enabled;
        }
        if let Some(auto) = module.get("autoStart").and_then(Value::as_bool) {
            app.auto_start = Some(auto);
        }
    }
    if let Some(wsl) = object.get("wslMemory") {
        settings.other.insert("wslMemory".into(), wsl.clone());
    }
    if object.contains_key("ui") {
        notes.push("the ui section (WinForms sizes) does not apply to the new tray".into());
    }
    Ok(Imported { settings, notes })
}

/// Imports the PowerShell tray's settings into `target`: backs up `target`
/// if it exists, then writes the result.
pub fn import_powershell_file(source: &Path, target: &Path) -> io::Result<Imported> {
    let text = std::fs::read_to_string(source)?;
    let base = load(target);
    if let Some(error) = base.error {
        return Err(io::Error::other(format!(
            "{} has an error ({error}); fix it first",
            target.display()
        )));
    }
    let imported = import_powershell(&text, base.settings).map_err(io::Error::other)?;
    if target.exists() {
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        std::fs::copy(
            target,
            target.with_extension(format!("pre-import-{stamp}.json")),
        )?;
    }
    save(target, &imported.settings)?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POWERSHELL_SCHEMA_5: &str = r#"{
      "schemaVersion": 5,
      "startup": { "launchOnWindowsStartup": true },
      "memory": { "windowsAmberBelowGiB": 3.5, "windowsRedBelowGiB": 1.25 },
      "ui": { "menuWidth": 230 },
      "modules": [
        { "id": "n8hotkeys", "manifest": ".\\n8hotkeys\\modular.json", "enabled": true, "autoStart": true },
        { "id": "n8voice", "manifest": ".\\n8voice\\modular.json", "enabled": false, "autoStart": false }
      ],
      "wslMemory": { "distro": "Ubuntu-24.04", "greenAboveGiB": 4, "redBelowGiB": 1.5 }
    }"#;

    #[test]
    fn imports_the_powershell_settings() {
        let text = format!("\u{feff}{POWERSHELL_SCHEMA_5}");
        let imported = import_powershell(&text, Settings::default()).unwrap();
        let s = &imported.settings;
        assert!(s.startup.launch_at_sign_in);
        assert_eq!(s.memory.attention_below_gib, 3.5);
        assert_eq!(s.memory.problem_below_gib, 1.25);
        assert_eq!(s.app("n8hotkeys").auto_start, Some(true));
        assert!(!s.app("voicedock").enabled, "n8voice is voicedock now");
        assert!(!s.apps.contains_key("n8voice"));
        assert_eq!(s.other["wslMemory"]["greenAboveGiB"], 4);
        assert!(!s.other.contains_key("ui"));
        assert!(imported.notes.iter().any(|n| n.contains("n8voice")));
    }

    #[test]
    fn refuses_a_newer_powershell_schema() {
        assert!(import_powershell(r#"{"schemaVersion": 6}"#, Settings::default()).is_err());
    }

    #[test]
    fn round_trips_and_keeps_unknown_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings
            .other
            .insert("future".into(), serde_json::json!({ "x": 1 }));
        settings.app_mut("fake").own_icon = true;
        save(&path, &settings).unwrap();
        let text = std::fs::read(&path).unwrap();
        assert_ne!(&text[..3], b"\xEF\xBB\xBF", "no BOM");
        let loaded = load(&path);
        assert_eq!(loaded.error, None);
        assert_eq!(loaded.settings, settings);
    }

    #[test]
    fn a_broken_file_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ broken").unwrap();
        let loaded = load(&path);
        assert!(loaded.error.is_some());
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.save().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    }

    #[test]
    fn import_backs_up_the_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("ps.json");
        let target = dir.path().join("settings.json");
        std::fs::write(&source, POWERSHELL_SCHEMA_5).unwrap();
        save(&target, &Settings::default()).unwrap();
        import_powershell_file(&source, &target).unwrap();
        assert!(load(&target).settings.startup.launch_at_sign_in);
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("pre-import")
            })
            .count();
        assert_eq!(backups, 1);
    }
}
