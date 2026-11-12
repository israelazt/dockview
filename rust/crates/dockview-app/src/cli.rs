//! Command-line options.
//!
//! ```text
//! dockview.exe [--test-pattern] [--ui-preview] [--theme auto|dark|light]
//!              [--lang auto|en|es] [--scale N] [--open-settings] [--stats]
//!              [--log-stats FILE.csv] [--video NAME]
//!              [--screenshot FILE.png]
//! ```

use std::path::PathBuf;

use crate::i18n::LangMode;
use crate::theme::ThemeMode;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cli {
    /// Show the synthetic colour bars instead of a capture device.
    pub test_pattern: bool,
    /// Show the interface mock-up over the test pattern; no devices are used.
    pub ui_preview: bool,
    /// Forces a theme for this run without changing the saved preference.
    pub theme: Option<ThemeMode>,
    /// Forces a language for this run without changing the saved preference.
    pub lang: Option<LangMode>,
    /// Overrides the Windows display scale for the interface and window size.
    pub scale: Option<f32>,
    /// Start with the settings popover open.
    pub open_settings: bool,
    /// Start with the instant-replay popover open.
    pub open_replay: bool,
    /// Start with the statistics panel open.
    pub show_stats: bool,
    /// Preview only: show sample problems in the diagnosis.
    pub demo_findings: bool,
    /// Append one line of measurements per second to this CSV file.
    pub log_stats: Option<PathBuf>,
    /// Open the first video device whose name contains this text.
    pub video: Option<String>,
    /// Render a few frames, save the window as a PNG and exit.
    pub screenshot: Option<PathBuf>,
}

impl Cli {
    /// Parses the arguments after the program name.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
                None => (arg, None),
            };
            let mut value = |name: &str| {
                inline
                    .clone()
                    .or_else(|| args.next())
                    .ok_or_else(|| format!("{name} needs a value"))
            };

            match flag.as_str() {
                "--test-pattern" => cli.test_pattern = true,
                "--ui-preview" => {
                    cli.ui_preview = true;
                    cli.test_pattern = true;
                }
                "--theme" => {
                    cli.theme = match value("--theme")?.to_lowercase().as_str() {
                        "auto" => Some(ThemeMode::Auto),
                        "dark" => Some(ThemeMode::Dark),
                        "light" => Some(ThemeMode::Light),
                        other => {
                            return Err(format!("unknown theme '{other}' (auto, dark, light)"))
                        }
                    }
                }
                "--lang" => {
                    cli.lang = match value("--lang")?.to_lowercase().as_str() {
                        "auto" => Some(LangMode::Auto),
                        "en" => Some(LangMode::En),
                        "es" => Some(LangMode::Es),
                        other => return Err(format!("unknown language '{other}' (auto, en, es)")),
                    }
                }
                "--scale" => {
                    let text = value("--scale")?;
                    let scale: f32 = text
                        .parse()
                        .map_err(|_| format!("invalid scale '{text}'"))?;
                    if !(0.5..=4.0).contains(&scale) {
                        return Err(format!("scale {scale} is outside 0.5–4.0"));
                    }
                    cli.scale = Some(scale);
                }
                "--open-settings" => cli.open_settings = true,
                "--open-replay" => cli.open_replay = true,
                "--stats" => cli.show_stats = true,
                "--video" => cli.video = Some(value("--video")?),
                "--log-stats" => cli.log_stats = Some(PathBuf::from(value("--log-stats")?)),
                "--demo-findings" => cli.demo_findings = true,
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                other => return Err(format!("unknown option '{other}'")),
            }
        }
        Ok(cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, String> {
        Cli::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_gives_defaults() {
        assert_eq!(parse(&[]).unwrap(), Cli::default());
    }

    #[test]
    fn ui_preview_implies_the_test_pattern() {
        let cli = parse(&["--ui-preview"]).unwrap();
        assert!(cli.ui_preview && cli.test_pattern);
    }

    #[test]
    fn values_can_be_separate_or_inline() {
        let a = parse(&["--theme", "light", "--lang", "es", "--scale", "1.5"]).unwrap();
        let b = parse(&["--theme=light", "--lang=es", "--scale=1.5"]).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.theme, Some(ThemeMode::Light));
        assert_eq!(a.lang, Some(LangMode::Es));
        assert_eq!(a.scale, Some(1.5));
    }

    #[test]
    fn open_settings_is_a_flag() {
        assert!(parse(&["--open-settings"]).unwrap().open_settings);
        assert!(!parse(&[]).unwrap().open_settings);
    }

    #[test]
    fn stats_is_a_flag() {
        assert!(parse(&["--stats"]).unwrap().show_stats);
        assert!(!parse(&[]).unwrap().show_stats);
    }

    #[test]
    fn video_takes_a_name_fragment() {
        assert_eq!(
            parse(&["--video", "ugreen"]).unwrap().video.as_deref(),
            Some("ugreen")
        );
        assert!(parse(&["--video"]).is_err());
    }

    #[test]
    fn log_stats_takes_a_path() {
        let cli = parse(&["--log-stats", "run.csv"]).unwrap();
        assert_eq!(cli.log_stats, Some(PathBuf::from("run.csv")));
        assert!(parse(&["--log-stats"]).is_err());
    }

    #[test]
    fn screenshot_takes_a_path() {
        let cli = parse(&["--screenshot", "out/shot.png"]).unwrap();
        assert_eq!(cli.screenshot, Some(PathBuf::from("out/shot.png")));
    }

    #[test]
    fn bad_input_is_reported() {
        assert!(parse(&["--theme", "purple"]).is_err());
        assert!(parse(&["--lang", "fr"]).is_err());
        assert!(parse(&["--scale", "abc"]).is_err());
        assert!(parse(&["--scale", "9"]).is_err());
        assert!(parse(&["--theme"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
