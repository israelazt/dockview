//! Going live: FLV packaging, the RTMP protocol and the service that keeps a
//! stream running, all over the [`transport::Transport`] port.

pub mod annexb;
pub mod flv;
pub mod rtmp;
pub mod transport;
