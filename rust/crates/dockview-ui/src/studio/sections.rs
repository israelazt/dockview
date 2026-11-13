//! The sections of the Studio panel and how they stack: each a header row and
//! a body that opens and closes. Pure arithmetic, tested without a window.

use dockview_domain::i18n::Key;

/// Height of a section's header row, in logical pixels.
pub const HEADER_H: f32 = 42.0;
/// Height of a section's body until its content is made (see the plan).
const BODY_H: f32 = 76.0;

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

    /// How tall its body is when fully open.
    pub fn body_height(self) -> f32 {
        BODY_H
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
}

impl SectionRow {
    /// Bottom of the whole section.
    pub fn bottom(&self) -> f32 {
        self.top + HEADER_H + self.body
    }
}

/// Stacks the sections. `expanded[i]` goes from 0 (folded) to 1 (open) for
/// `StudioSection::ALL[i]`, so the folding can be animated. Returns the rows
/// and the height of the whole list.
pub fn stack(expanded: &[f32; 6]) -> (Vec<SectionRow>, f32) {
    let mut rows = Vec::with_capacity(StudioSection::ALL.len());
    let mut top = 0.0;
    for (section, open) in StudioSection::ALL.into_iter().zip(expanded) {
        let row = SectionRow {
            section,
            top,
            body: (section.body_height() * open.clamp(0.0, 1.0)).round(),
        };
        top = row.bottom();
        rows.push(row);
    }
    (rows, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_sections_are_just_their_headers() {
        let (rows, height) = stack(&[0.0; 6]);
        assert_eq!(rows.len(), 6);
        assert_eq!(height, HEADER_H * 6.0);
        assert_eq!(rows[2].top, HEADER_H * 2.0);
        assert!(rows.iter().all(|r| r.body == 0.0));
    }

    #[test]
    fn opening_a_section_pushes_the_ones_below_it_down() {
        let mut open = [0.0; 6];
        open[1] = 1.0;
        let (rows, height) = stack(&open);
        assert_eq!(rows[1].body, BODY_H);
        assert_eq!(rows[2].top, rows[1].bottom());
        assert_eq!(rows[2].top, HEADER_H * 2.0 + BODY_H);
        assert_eq!(height, HEADER_H * 6.0 + BODY_H);
    }

    #[test]
    fn a_half_open_section_is_half_as_tall() {
        let mut open = [0.0; 6];
        open[0] = 0.5;
        let (rows, _) = stack(&open);
        assert_eq!(rows[0].body, (BODY_H / 2.0).round());
    }

    #[test]
    fn out_of_range_amounts_are_kept_in_range() {
        let (rows, _) = stack(&[2.0, -1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(rows[0].body, BODY_H);
        assert_eq!(rows[1].body, 0.0);
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
        };
        assert_eq!(row.bottom(), 100.0 + HEADER_H + 30.0);
    }
}
