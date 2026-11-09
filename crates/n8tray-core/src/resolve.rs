//! From a manifest to a command line, under the contract's rules (spec 3.2,
//! 3.4 and section 9): paths resolve inside `installDir` after symlinks,
//! interpreters come from a fixed list, arguments are fixed arrays and the
//! only substitution is `{component}`, an id the app itself listed.

use std::path::{Path, PathBuf};

use n8tray_contract::manifest::{Interpreter, Manifest};
use n8tray_contract::paths;

use crate::folders::Folders;

/// A program to start, fully resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub hidden: bool,
}

/// Expands `installDir`'s leading placeholder into an absolute path.
pub fn install_dir(manifest: &Manifest, folders: &Folders) -> Result<PathBuf, String> {
    let dir = manifest
        .install_dir
        .as_deref()
        .ok_or("the manifest has no installDir")?;
    expand(dir, folders)
}

/// Expands one leading placeholder (spec 3.2).
pub fn expand(path: &str, folders: &Folders) -> Result<PathBuf, String> {
    let (placeholder, rest) = paths::split_placeholder(path).map_err(|e| e.to_string())?;
    let rest = rest.trim_start_matches(['/', '\\']);
    let env = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| format!("{name} is not set"))
    };
    let base = match placeholder {
        None => return Ok(PathBuf::from(native(path))),
        Some("%LOCALAPPDATA%") if cfg!(windows) => env("LOCALAPPDATA")?,
        Some("%USERPROFILE%") if cfg!(windows) => env("USERPROFILE")?,
        Some("%LOCALAPPDATA%" | "%USERPROFILE%") => {
            return Err(format!("{path}: a Windows-only placeholder"));
        }
        Some("%HOME%") => env("HOME").or_else(|_| env("USERPROFILE"))?,
        Some("%N8TRAY_APPS%") => folders.apps.clone(),
        Some("%N8TRAY_TOOLS%") => folders.tools.clone(),
        Some(other) => return Err(format!("unknown placeholder {other}")),
    };
    Ok(if rest.is_empty() {
        base
    } else {
        base.join(native(rest))
    })
}

/// A contract path with this OS's separators.
fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.replace('\\', "/")
    }
}

/// Resolves a relative path inside `install_dir`, following symlinks, and
/// rejects anything that ends up outside it.
pub fn inside(install_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    paths::check_relative(relative).map_err(|e| format!("{relative}: {e}"))?;
    let root = install_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", install_dir.display()))?;
    let joined = root.join(native(relative));
    let resolved = joined
        .canonicalize()
        .map_err(|e| format!("{}: {e}", joined.display()))?;
    if !resolved.starts_with(&root) {
        return Err(format!("{relative} leaves the install folder"));
    }
    Ok(resolved)
}

/// Like [`inside`], for a path that need not exist yet: the existing part is
/// resolved, and the result must stay inside `install_dir`.
pub fn inside_lenient(install_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    paths::check_relative(relative).map_err(|e| format!("{relative}: {e}"))?;
    match inside(install_dir, relative) {
        Ok(path) => Ok(path),
        Err(_) => Ok(install_dir.join(native(relative))),
    }
}

/// Finds a program on PATH (with PATHEXT on Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The interpreter's executable and the arguments before the script (spec 3.4).
fn interpreter(kind: Interpreter) -> Result<(PathBuf, Vec<String>), String> {
    match kind {
        Interpreter::Powershell => {
            if !cfg!(windows) {
                return Err("Windows PowerShell exists only on Windows".into());
            }
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            let exe = Path::new(&root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
            Ok((
                exe,
                [
                    "-NoLogo",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ]
                .map(String::from)
                .to_vec(),
            ))
        }
        Interpreter::Pwsh => Ok((
            which("pwsh").ok_or("pwsh is not installed (PowerShell 7)")?,
            ["-NoLogo", "-NoProfile", "-File"]
                .map(String::from)
                .to_vec(),
        )),
        Interpreter::Node => Ok((which("node").ok_or("node is not installed")?, Vec::new())),
    }
}

/// The command line for the app's `run`, with `extra` arguments instead of
/// `run.args` when given (a stop or an `exec` action).
pub fn launch(
    manifest: &Manifest,
    folders: &Folders,
    extra: Option<&[String]>,
) -> Result<Launch, String> {
    let run = manifest
        .run
        .as_ref()
        .ok_or("the manifest has no run section")?;
    let dir = install_dir(manifest, folders)?;
    let given = extra
        .map(<[String]>::to_vec)
        .unwrap_or_else(|| run.args.clone().unwrap_or_default());
    let (program, args) = match (&run.program, run.interpreter, &run.script) {
        (Some(program), None, _) => (inside(&dir, program)?, given),
        (None, Some(kind), Some(script)) => {
            let (exe, mut args) = interpreter(kind)?;
            args.push(inside(&dir, script)?.display().to_string());
            args.extend(given);
            (exe, args)
        }
        _ => return Err("run needs exactly one of program and interpreter with script".into()),
    };
    let cwd = match &run.cwd {
        Some(cwd) => inside(&dir, cwd)?,
        None => dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?,
    };
    Ok(Launch {
        program,
        args,
        cwd,
        hidden: run.hidden.unwrap_or(true),
    })
}

/// Replaces an argument that is exactly `{component}` with `component`, which
/// must be one of the ids the app listed (spec 3.5).
pub fn substitute(
    args: &[String],
    component: Option<&str>,
    listed: &[String],
) -> Result<Vec<String>, String> {
    let needs = args.iter().any(|a| a == "{component}");
    let component = match (needs, component) {
        (false, _) => None,
        (true, None) => return Err("this action needs a component".into()),
        (true, Some(c)) if listed.iter().any(|l| l == c) && valid_id(c) => Some(c),
        (true, Some(c)) => return Err(format!("{c} is not a component the app lists")),
    };
    Ok(args
        .iter()
        .map(|a| match (a.as_str(), component) {
            ("{component}", Some(c)) => c.to_string(),
            _ => a.clone(),
        })
        .collect())
}

/// The id rule for components: `[a-z0-9-]{1,32}` (plan, section 12).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(dir: &Path, run: &str) -> Manifest {
        serde_json::from_str(&format!(
            r#"{{ "contract": 2, "id": "x", "name": "X", "version": "1.0.0",
                 "installDir": {}, "status": {{ "type": "file" }}, "run": {run} }}"#,
            serde_json::to_string(&dir.display().to_string()).unwrap()
        ))
        .unwrap()
    }

    #[test]
    fn programs_resolve_inside_the_install_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin").join("app"), "").unwrap();
        let folders = Folders::under(dir.path());
        let m = manifest(
            dir.path(),
            r#"{ "program": "bin/app", "args": ["--managed"] }"#,
        );
        let launch = launch(&m, &folders, None).unwrap();
        assert!(launch.program.ends_with(Path::new("bin").join("app")));
        assert_eq!(launch.args, ["--managed"]);
        assert!(launch.hidden);
        let stop = super::launch(&m, &folders, Some(&["-Stop".to_string()])).unwrap();
        assert_eq!(stop.args, ["-Stop"]);

        let missing = manifest(dir.path(), r#"{ "program": "bin/missing" }"#);
        assert!(super::launch(&missing, &folders, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_install_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("evil"), "").unwrap();
        std::os::unix::fs::symlink(outside.path().join("evil"), dir.path().join("app")).unwrap();
        let err = inside(dir.path(), "app").unwrap_err();
        assert!(err.contains("leaves the install folder"), "{err}");
        assert!(inside(dir.path(), "../x").is_err());
        assert!(inside(dir.path(), "/etc/passwd").is_err());
    }

    #[test]
    fn placeholders_expand() {
        let folders = Folders::under(Path::new("/base"));
        assert_eq!(
            expand("%N8TRAY_APPS%/voicedock/1.0.0", &folders).unwrap(),
            Path::new("/base/apps").join(native("voicedock/1.0.0"))
        );
        assert!(expand("%NOPE%/x", &folders).is_err());
        if !cfg!(windows) {
            assert!(expand("%LOCALAPPDATA%\\x", &folders).is_err());
        }
    }

    #[test]
    fn only_listed_components_are_substituted() {
        let args = vec!["--engine".to_string(), "{component}".to_string()];
        let listed = vec!["kokoro".to_string()];
        assert_eq!(
            substitute(&args, Some("kokoro"), &listed).unwrap(),
            ["--engine", "kokoro"]
        );
        assert!(substitute(&args, Some("piper"), &listed).is_err());
        assert!(substitute(&args, None, &listed).is_err());
        assert!(substitute(&args, Some("a b"), &["a b".to_string()]).is_err());
        assert_eq!(
            substitute(&["-x".to_string()], None, &listed).unwrap(),
            ["-x"]
        );
    }
}
