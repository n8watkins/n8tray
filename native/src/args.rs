//! Command-line options: the same as the Tauri app's (app/src-tauri/src/main.rs).
#![forbid(unsafe_code)]

/// What N8 Tray was started with.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Args {
    /// Ask the running N8 Tray to exit (apps keep running).
    pub exit: bool,
    /// Started at sign-in.
    pub autostart: bool,
    /// Close the running N8 Tray's window.
    pub hide: bool,
    /// Open the window once started.
    pub show: bool,
    /// Wait for this process to exit first (Restart N8 Tray).
    pub restart_after: Option<u32>,
    /// Import the PowerShell N8 Tray's settings file, then exit.
    pub import_settings: Option<String>,
}

/// Reads the arguments after the program's name. Unknown ones are ignored,
/// as before.
pub fn parse(args: impl IntoIterator<Item = String>) -> Args {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--exit" | "-Exit" => out.exit = true,
            "--autostart" => out.autostart = true,
            "--hide" => out.hide = true,
            "--show" => out.show = true,
            "--restart-after" => out.restart_after = it.next().and_then(|p| p.parse().ok()),
            "--import-settings" => out.import_settings = it.next(),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Args {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn nothing_given_is_a_plain_start() {
        assert_eq!(parse_str(""), Args::default());
    }

    #[test]
    fn each_flag_is_read() {
        assert!(parse_str("--exit").exit);
        assert!(parse_str("-Exit").exit, "the PowerShell tray's spelling");
        assert!(parse_str("--autostart").autostart);
        assert!(parse_str("--hide").hide);
        assert!(parse_str("--show").show);
        let all = parse_str("--autostart --show --hide --exit");
        assert!(all.autostart && all.show && all.hide && all.exit);
    }

    #[test]
    fn values_follow_their_flag() {
        assert_eq!(parse_str("--restart-after 4242").restart_after, Some(4242));
        assert_eq!(parse_str("--restart-after soon").restart_after, None);
        assert_eq!(parse_str("--restart-after").restart_after, None);
        assert_eq!(
            parse_str("--import-settings C:\\old\\settings.json").import_settings,
            Some("C:\\old\\settings.json".into())
        );
        assert_eq!(parse_str("--import-settings").import_settings, None);
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        let args = parse_str("--verbose --show extra");
        assert_eq!(
            args,
            Args {
                show: true,
                ..Args::default()
            }
        );
    }
}
