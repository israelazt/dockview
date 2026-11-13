//! A compressed video picture, as the encoder hands it over and the replay
//! buffer and the MP4 writer take it.

/// One compressed H.264 access unit (Annex B), ready to be stored or muxed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    /// Presentation time in 100 ns units.
    pub timestamp: i64,
    /// Duration in 100 ns units.
    pub duration: i64,
    /// A clean point: decoding can start here.
    pub keyframe: bool,
}
