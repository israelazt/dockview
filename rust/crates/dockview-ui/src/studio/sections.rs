//! The sections of the Studio panel and how they stack: each a header row and
//! a body that opens and closes. Pure arithmetic, tested without a window.

use dockview_domain::i18n::Key;
use dockview_domain::stats::Stats;
use dockview_domain::view::Model;

use crate::Ui;

/// Height of a section's header row, in logical pixels.
pub const HEADER_H: f32 = 42.0;

/// Which sections a panel lists, and whether it has a header of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every section, under a header with the close button.
    Full,
    /// The right panel when the left column exists: everything but the
    /// scenes and the sources.
    Main,
    /// The left column: the scenes and the sources, with no header.
    Side,
}

impl Scope {
    /// Whether this panel lists `section`.
    pub fn includes(self, section: StudioSection) -> bool {
        let side = matches!(section, StudioSection::Sources | StudioSection::Scenes);
        match self {
            Self::Full => true,
            Self::Main => !side,
            Self::Side => side,
        }
    }

    /// Whether each section is a card of its own, always open (the left
    /// column), instead of a fold in one list.
    pub fn cards(self) -> bool {
        self == Self::Side
    }

    /// Air between the sections, in logical pixels.
    pub fn gap(self) -> f32 {
        if self.cards() {
            12.0
        } else {
            0.0
        }
    }

    /// Height of the panel's own header.
    pub fn top(self) -> f32 {
        if self == Self::Side {
            0.0
        } else {
            super::TOP_H
        }
    }
}

/// One of the sections of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StudioSection {
    Sources,
    Scenes,
    Audio,
    Recording,
    Settings,
    Stats,
}

impl StudioSection {
    /// In the order they are listed.
    pub const ALL: [StudioSection; 6] = [
        Self::Sources,
        Self::Scenes,
        Self::Audio,
        Self::Recording,
        Self::Settings,
        Self::Stats,
    ];

    /// The section called `name` on the command line (case does not matter).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sources" => Some(Self::Sources),
            "scenes" => Some(Self::Scenes),
            "audio" => Some(Self::Audio),
            "recording" => Some(Self::Recording),
            "settings" => Some(Self::Settings),
            "stats" => Some(Self::Stats),
            _ => None,
        }
    }

    /// The words on its header.
    pub fn title(self) -> Key {
        match self {
            Self::Sources => Key::StudioSources,
            Self::Scenes => Key::StudioScenes,
            Self::Audio => Key::StudioAudio,
            Self::Recording => Key::StudioRecording,
            Self::Settings => Key::StudioSettings,
            Self::Stats => Key::StudioStats,
        }
    }

    /// How tall its body is when fully open (it can depend on what the model
    /// lists, such as how many devices there are, and on how the text of the
    /// statistics wraps in a panel `width` wide).
    pub fn body_height(self, ui: &mut Ui, m: &Model, stats: &Stats, width: f32) -> f32 {
        self.body_height_in(Scope::Full, ui, m, stats, width)
    }

    /// The same, in a panel with this `scope` (the scenes have a "+" in the
    /// header of their card instead of a row of their own).
    pub fn body_height_in(
        self,
        scope: Scope,
        ui: &mut Ui,
        m: &Model,
        stats: &Stats,
        width: f32,
    ) -> f32 {
        match self {
            Self::Settings => super::settings::body_height(),
            Self::Sources => super::sources::body_height(m),
            Self::Audio => super::audio::body_height(m),
            Self::Scenes => super::scenes::body_height_in(m, scope.cards()),
            Self::Recording => super::recording::body_height(ui, m, width),
            Self::Stats => super::stats::body_height(ui, m, stats, width),
        }
    }
}

/// Where a section sits in the list, relative to the top of the list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionRow {
    pub section: StudioSection,
    /// Top of the header row.
    pub top: f32,
    /// Height of the body now (between 0, folded, and its full height).
    pub body: f32,
    /// Height of the body when fully open.
    pub full: f32,
}

impl SectionRow {
    /// Bottom of the whole section.
    pub fn bottom(&self) -> f32 {
        self.top + HEADER_H + self.body
    }
}

/// Stacks the sections. `expanded[i]` goes from 0 (folded) to 1 (open) for
/// `StudioSection::ALL[i]`, so the folding can be animated, and `heights[i]`
/// is how tall its body is when open. Only the sections `scope` lists are
/// stacked. Returns the rows and the height of the whole list.
pub fn stack(
    scope: Scope,
    expanded: &[f32; 6],
    heights: &[f32; 6],
) -> (Vec<SectionRow>, f32) {
    let mut rows = Vec::with_capacity(StudioSection::ALL.len());
    let mut top = 0.0;
    // The cards of the left column list the scenes first.
    let mut order: Vec<usize> = (0..StudioSection::ALL.len())
        .filter(|i| scope.includes(StudioSection::ALL[*i]))
        .collect();
    if scope.cards() {
        order.sort_by_key(|i| StudioSection::ALL[*i] != StudioSection::Scenes);
    }
    for i in order {
        let row = SectionRow {
            section: StudioSection::ALL[i],
            top,
            body: (heights[i] * expanded[i].clamp(0.0, 1.0)).round(),
            full: heights[i],
        };
        top = row.bottom() + scope.gap();
        rows.push(row);
    }
    if !rows.is_empty() {
        top -= scope.gap();
    }
    (rows, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A height to stack sections with, in the tests of the stacking.
    const BODY_H: f32 = 76.0;

    #[test]
    fn each_scope_lists_its_own_sections() {
        let count = |scope: Scope| {
            StudioSection::ALL
                .iter()
                .filter(|s| scope.includes(**s))
                .count()
        };
        assert_eq!(count(Scope::Full), 6);
        assert_eq!(count(Scope::Main), 4);
        assert_eq!(count(Scope::Side), 2);
        for s in StudioSection::ALL {
            assert_ne!(Scope::Main.includes(s), Scope::Side.includes(s), "{s:?}");
        }
        assert_eq!(Scope::Side.top(), 0.0);
        assert!(Scope::Main.top() > 0.0);
    }

    #[test]
    fn the_side_cards_are_apart_and_the_scenes_come_first() {
        let (rows, height) = stack(Scope::Side, &[1.0; 6], &[BODY_H; 6]);
        assert_eq!(rows[0].section, StudioSection::Scenes);
        assert_eq!(rows[1].section, StudioSection::Sources);
        assert_eq!(rows[1].top, rows[0].bottom() + Scope::Side.gap());
        assert_eq!(height, rows[1].bottom(), "no air after the last card");
        assert!(Scope::Side.cards() && !Scope::Main.cards());
    }

    #[test]
    fn a_scope_stacks_only_its_sections() {
        let (rows, height) = stack(Scope::Side, &[0.0; 6], &[BODY_H; 6]);
        assert_eq!(rows.len(), 2);
        assert_eq!(height, HEADER_H * 2.0 + Scope::Side.gap());
        let (rows, _) = stack(Scope::Main, &[0.0; 6], &[BODY_H; 6]);
        assert!(rows.iter().all(|r| !matches!(
            r.section,
            StudioSection::Sources | StudioSection::Scenes
        )));
        assert_eq!(rows[0].top, 0.0);
    }

    #[test]
    fn folded_sections_are_just_their_headers() {
        let (rows, height) = stack(Scope::Full, &[0.0; 6], &[BODY_H; 6]);
        assert_eq!(rows.len(), 6);
        assert_eq!(height, HEADER_H * 6.0);
        assert_eq!(rows[2].top, HEADER_H * 2.0);
        assert!(rows.iter().all(|r| r.body == 0.0));
    }

    #[test]
    fn opening_a_section_pushes_the_ones_below_it_down() {
        let mut open = [0.0; 6];
        open[1] = 1.0;
        let (rows, height) = stack(Scope::Full, &open, &[BODY_H; 6]);
        assert_eq!(rows[1].body, BODY_H);
        assert_eq!(rows[2].top, rows[1].bottom());
        assert_eq!(rows[2].top, HEADER_H * 2.0 + BODY_H);
        assert_eq!(height, HEADER_H * 6.0 + BODY_H);
    }

    #[test]
    fn a_half_open_section_is_half_as_tall() {
        let mut open = [0.0; 6];
        open[0] = 0.5;
        let (rows, _) = stack(Scope::Full, &open, &[BODY_H; 6]);
        assert_eq!(rows[0].body, (BODY_H / 2.0).round());
    }

    #[test]
    fn out_of_range_amounts_are_kept_in_range() {
        let (rows, _) = stack(Scope::Full, &[2.0, -1.0, 0.0, 0.0, 0.0, 0.0], &[BODY_H; 6]);
        assert_eq!(rows[0].body, BODY_H);
        assert_eq!(rows[1].body, 0.0);
    }

    #[test]
    fn each_section_can_have_its_own_height() {
        let mut heights = [BODY_H; 6];
        heights[0] = 200.0;
        let (rows, height) = stack(Scope::Full, &[1.0; 6], &heights);
        assert_eq!(rows[0].body, 200.0);
        assert_eq!(rows[0].full, 200.0);
        assert_eq!(rows[1].top, HEADER_H + 200.0);
        assert_eq!(height, HEADER_H * 6.0 + 200.0 + BODY_H * 5.0);
        // Folded, a tall body takes no room however tall it would be.
        let (rows, _) = stack(Scope::Full, &[0.0; 6], &heights);
        assert_eq!((rows[0].body, rows[0].full), (0.0, 200.0));
    }

    #[test]
    fn sections_can_be_named_on_the_command_line() {
        assert_eq!(
            StudioSection::from_name("Settings"),
            Some(StudioSection::Settings)
        );
        assert_eq!(
            StudioSection::from_name("STATS"),
            Some(StudioSection::Stats)
        );
        assert_eq!(StudioSection::from_name("nope"), None);
        for section in StudioSection::ALL {
            let name = format!("{section:?}");
            assert_eq!(StudioSection::from_name(&name), Some(section), "{name}");
        }
    }

    #[test]
    fn every_section_has_its_own_title() {
        let titles: Vec<_> = StudioSection::ALL.iter().map(|s| s.title()).collect();
        for (i, a) in titles.iter().enumerate() {
            for b in &titles[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_row_knows_where_it_ends() {
        let row = SectionRow {
            section: StudioSection::Audio,
            top: 100.0,
            body: 30.0,
            full: 30.0,
        };
        assert_eq!(row.bottom(), 100.0 + HEADER_H + 30.0);
    }
}
