//! Command-line options.
//!
//! ```text
//! dockview.exe [--test-pattern] [--ui-preview] [--theme auto|dark|light]
//!              [--lang auto|en|es] [--scale N] [--open-settings] [--open-studio]
//!              [--stats]
//!              [--log-stats FILE.csv] [--video NAME]
//!              [--screenshot FILE.png] [--pip SOURCE] [--pip-size N]
//! ```
//!
//! `--pip` draws a second source in the bottom-right corner of the picture
//! (a scene of two layers): `video:NAME`, `monitor:N` or `window:TEXT`.

use std::path::PathBuf;

use dockview_domain::i18n::LangMode;
use dockview_domain::theme::ThemeMode;

/// A source to draw in the corner of the picture.
#[derive(Debug, Clone, PartialEq)]
pub enum PipSpec {
    /// The capture device whose name contains this text.
    Video(String),
    /// The monitor at this position (0 is the primary).
    Monitor(usize),
    /// The first window whose title contains this text.
    Window(String),
}

impl PipSpec {
    /// Parses `kind:value`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (kind, value) = text
            .split_once(':')
            .ok_or_else(|| format!("--pip needs kind:value, e.g. monitor:1 (got '{text}')"))?;
        if value.is_empty() {
            return Err(format!("--pip {kind}: needs a value"));
        }
        match kind {
            "video" => Ok(Self::Video(value.to_owned())),
            "window" => Ok(Self::Window(value.to_owned())),
            "monitor" => value
                .parse()
                .map(Self::Monitor)
                .map_err(|_| format!("--pip monitor: '{value}' is not a number")),
            other => Err(format!(
                "--pip: unknown kind '{other}' (use video, monitor or window)"
            )),
        }
    }
}

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
    /// Start with the source popover open.
    pub open_sources: bool,
    /// Start with the scenes popover open.
    pub open_scenes: bool,
    /// Start with the Studio side panel open.
    pub open_studio: bool,
    /// Start with this section of the Studio panel unfolded (`sources`,
    /// `scenes`, `audio`, `recording`, `settings` or `stats`).
    pub studio_section: Option<String>,
    /// Start with the microphone on (for tests and screenshots; it is off by default).
    pub mic: bool,
    /// Start recording right away (for tests and screenshots).
    pub record: bool,
    /// Start by capturing this monitor (0 is the primary) instead of the card.
    pub screen: Option<usize>,
    /// Start by capturing the first window whose title contains this text.
    pub capture_window: Option<String>,
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
    /// A second source drawn in the bottom-right corner (a two-layer scene).
    pub pip: Option<PipSpec>,
    /// Width of that corner picture as a fraction of the whole (default 0.28).
    pub pip_size: Option<f32>,
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
                "--open-sources" => cli.open_sources = true,
                "--open-scenes" => cli.open_scenes = true,
                "--open-studio" => cli.open_studio = true,
                "--studio-section" => {
                    cli.studio_section = Some(value("--studio-section")?);
                    cli.open_studio = true;
                }
                "--mic" => cli.mic = true,
                "--record" => cli.record = true,
                "--screen" => {
                    cli.screen = Some(
                        value("--screen")?
                            .parse()
                            .map_err(|_| "--screen needs a number".to_string())?,
                    )
                }
                "--window" => cli.capture_window = Some(value("--window")?),
                "--stats" => cli.show_stats = true,
                "--video" => cli.video = Some(value("--video")?),
                "--log-stats" => cli.log_stats = Some(PathBuf::from(value("--log-stats")?)),
                "--demo-findings" => cli.demo_findings = true,
                "--screenshot" => cli.screenshot = Some(PathBuf::from(value("--screenshot")?)),
                "--pip" => cli.pip = Some(PipSpec::parse(&value("--pip")?)?),
                "--pip-size" => {
                    let text = value("--pip-size")?;
                    cli.pip_size = Some(
                        text.parse::<f32>()
                            .ok()
                            .filter(|v| (0.1..=0.6).contains(v))
                            .ok_or_else(|| {
                                format!("--pip-size needs a number from 0.1 to 0.6, got '{text}'")
                            })?,
                    );
                }
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
    fn pip_takes_a_kind_and_a_value() {
        assert_eq!(
            parse(&["--pip", "video:C7"]).unwrap().pip,
            Some(PipSpec::Video("C7".into()))
        );
        assert_eq!(
            parse(&["--pip=monitor:1"]).unwrap().pip,
            Some(PipSpec::Monitor(1))
        );
        assert_eq!(
            parse(&["--pip", "window:Mi juego"]).unwrap().pip,
            Some(PipSpec::Window("Mi juego".into()))
        );
        assert!(parse(&["--pip", "monitor"]).is_err());
        assert!(parse(&["--pip", "monitor:x"]).is_err());
        assert!(parse(&["--pip", "cosa:1"]).is_err());
        assert!(parse(&["--pip", "video:"]).is_err());
        assert!(parse(&["--pip"]).is_err());
    }

    #[test]
    fn pip_size_must_be_a_sensible_fraction() {
        assert_eq!(parse(&["--pip-size", "0.3"]).unwrap().pip_size, Some(0.3));
        assert!(parse(&["--pip-size", "0.05"]).is_err());
        assert!(parse(&["--pip-size", "2"]).is_err());
        assert!(parse(&["--pip-size", "big"]).is_err());
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
    fn open_studio_is_a_flag() {
        assert!(parse(&["--open-studio"]).unwrap().open_studio);
        assert!(!parse(&[]).unwrap().open_studio);
    }

    #[test]
    fn a_studio_section_opens_the_panel_too_and_needs_a_name() {
        let cli = parse(&["--studio-section", "audio"]).unwrap();
        assert_eq!(cli.studio_section.as_deref(), Some("audio"));
        assert!(cli.open_studio);
        assert!(parse(&["--studio-section"]).is_err());
        assert_eq!(parse(&[]).unwrap().studio_section, None);
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
