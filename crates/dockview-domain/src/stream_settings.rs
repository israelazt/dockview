//! What the user chose about going live, saved with the rest of the settings.
//! The stream key is a secret and is not here: it is kept apart (see
//! [`crate::broadcast::SecretStore`]).

use serde::{Deserialize, Serialize};

use crate::broadcast::StreamRequest;

pub const MIN_FPS: u32 = 10;
pub const MAX_FPS: u32 = 120;
pub const MIN_KBPS: u32 = 500;
pub const MAX_KBPS: u32 = 51_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamSettings {
    /// The ingest address, `rtmp://…` or `rtmps://…`. Empty until chosen.
    pub url: String,
    pub fps: u32,
    /// Video bitrate in kilobits per second.
    pub video_kbps: u32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            fps: 30,
            video_kbps: 4500,
        }
    }
}

impl StreamSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Values from an old or hand-edited file, brought back to sensible ones.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.url = self.url.trim().to_string();
        self.fps = self.fps.clamp(MIN_FPS, MAX_FPS);
        self.video_kbps = self.video_kbps.clamp(MIN_KBPS, MAX_KBPS);
        self
    }

    /// The request to start a stream with `key`, or why it cannot be made.
    pub fn request(&self, key: &str) -> Result<StreamRequest, StreamSetupError> {
        let url = self.url.trim();
        if !(url.starts_with("rtmp://") || url.starts_with("rtmps://")) || url.len() < 12 {
            return Err(StreamSetupError::BadAddress);
        }
        if key.trim().is_empty() {
            return Err(StreamSetupError::NoKey);
        }
        Ok(StreamRequest {
            url: url.to_string(),
            key: key.trim().to_string(),
            fps: self.fps.clamp(MIN_FPS, MAX_FPS),
            video_kbps: self.video_kbps.clamp(MIN_KBPS, MAX_KBPS),
        })
    }
}

/// Why a stream cannot be started with what the user has set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSetupError {
    /// The address is empty or not an `rtmp://` / `rtmps://` one.
    BadAddress,
    /// There is no stream key.
    NoKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(url: &str) -> StreamSettings {
        StreamSettings {
            url: url.into(),
            ..StreamSettings::default()
        }
    }

    #[test]
    fn a_request_needs_an_rtmp_address_and_a_key() {
        let ok = set("rtmps://a.rtmps.youtube.com:443/live2");
        assert_eq!(ok.request("  abc ").unwrap().key, "abc");
        assert_eq!(set("").request("k"), Err(StreamSetupError::BadAddress));
        assert_eq!(
            set("http://x.example/app").request("k"),
            Err(StreamSetupError::BadAddress)
        );
        assert_eq!(ok.request("   "), Err(StreamSetupError::NoKey));
    }

    #[test]
    fn numbers_from_a_file_are_brought_into_range() {
        let s = StreamSettings {
            url: " rtmp://h/a ".into(),
            fps: 0,
            video_kbps: 9_000_000,
        }
        .sanitized();
        assert_eq!(
            (s.url.as_str(), s.fps, s.video_kbps),
            ("rtmp://h/a", MIN_FPS, MAX_KBPS)
        );
    }

    #[test]
    fn missing_fields_fall_back_and_defaults_are_not_saved() {
        let s: StreamSettings = serde_json::from_str(r#"{"url":"rtmp://h/app"}"#).unwrap();
        assert_eq!((s.fps, s.video_kbps), (30, 4500));
        assert!(StreamSettings::default().is_default());
        assert!(!s.is_default());
    }
}
