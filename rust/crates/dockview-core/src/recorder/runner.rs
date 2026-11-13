//! The recorder thread: takes frames and audio from the taps, encodes them and
//! keeps the replay ring and the recording session going.

use super::session::*;
use super::*;

/// Recorder thread body.
pub(super) fn run(
    tap: &FrameTap,
    audio: &AudioTap,
    mic: &AudioTap,
    shared: &Shared,
    stop: &AtomicBool,
    settings: ReplaySettings,
) {
    let Ok(session) = MfSession::start() else {
        shared.set_status(RecorderStatus::Failed(
            "Media Foundation is unavailable".into(),
        ));
        return;
    };
    let mut encoder: Option<(H264Encoder, EncodeSettings)> = None;
    let mut scratch: Vec<u8> = Vec::new();
    let mut rec = RecCtx::default();

    while !stop.load(Ordering::Relaxed) {
        drain_audio(audio, mic, shared);
        rec_commands(&mut rec, shared);
        rec_pump(&mut rec, shared);
        let Some(frame) = tap.take(POLL) else {
            continue;
        };
        if frame.format != PixelFormat::Nv12 {
            shared.set_status(RecorderStatus::Unsupported(
                "only NV12 video can be recorded".into(),
            ));
            tap.recycle(frame);
            continue;
        }

        let wanted = EncodeSettings {
            width: frame.width,
            height: frame.height,
            fps: settings.fps,
            bitrate_bps: settings.bitrate_mbps.saturating_mul(1_000_000),
        };
        if encoder
            .as_ref()
            .is_none_or(|(_, current)| *current != wanted)
        {
            // A new stream (first frame, or the resolution changed). A file
            // cannot change size halfway, so a running recording ends here.
            if let Some(active) = rec.active.take() {
                rec_finish(active, shared, EndReason::ResolutionChanged, true);
                rec.part += 1;
                if let Some(base) = &rec.base {
                    rec.pending = Some(part_path(base, rec.part));
                    rec.pending_since = Some(Instant::now());
                }
            }
            match H264Encoder::new(&session, &wanted) {
                Ok(e) => {
                    encoder = Some((e, wanted));
                    *shared.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(wanted);
                    *shared.header.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    if let Some(ring) = shared
                        .ring
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_mut()
                    {
                        ring.clear();
                    }
                }
                Err(e) => {
                    shared.set_status(RecorderStatus::Unsupported(e.message()));
                    tap.recycle(frame);
                    thread::sleep(POLL);
                    continue;
                }
            }
        }
        let Some((encoder, _)) = encoder.as_mut() else {
            tap.recycle(frame);
            continue;
        };

        let timestamp = (frame
            .arrived
            .saturating_duration_since(shared.epoch)
            .as_nanos()
            / 100) as i64;
        match encode_frame(encoder, &frame, timestamp, &mut scratch) {
            Ok(out) => {
                for encoded in out {
                    if encoded.keyframe
                        && shared
                            .header
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .is_none()
                    {
                        let header = encoder.output_type().ok().and_then(|t| sequence_header(&t));
                        *shared.header.lock().unwrap_or_else(|e| e.into_inner()) = header;
                    }
                    shared.encoded_frames.fetch_add(1, Ordering::Relaxed);
                    rec_on_frame(&mut rec, shared, &session, &wanted, &encoded);
                    if let Some(ring) = shared
                        .ring
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_mut()
                    {
                        ring.push(encoded);
                    }
                }
                shared.set_status(RecorderStatus::Recording);
            }
            Err(e) => shared.set_status(RecorderStatus::Failed(e.message())),
        }
        tap.recycle(frame);
    }
    // Closing the program while recording still finishes the file.
    if let Some(active) = rec.active.take() {
        rec_finish(active, shared, EndReason::Stopped, false);
    } else if rec.pending.take().is_some() {
        rec_end(
            shared,
            Err("stopped before the first frame".into()),
            EndReason::Cancelled,
            false,
        );
    }
    if let Some((mut encoder, _)) = encoder {
        let _ = encoder.flush();
    }
}

/// Moves every queued audio block onto the shared timeline.
pub(super) fn drain_audio(audio: &AudioTap, mic: &AudioTap, shared: &Shared) {
    let to_ticks =
        |start: Instant| (start.saturating_duration_since(shared.epoch).as_nanos() / 100) as i64;
    {
        let mut pcm = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(chunk) = mic.take(Duration::ZERO) {
            pcm.push(to_ticks(chunk.start), &chunk.samples);
        }
    }
    let offset = i64::from(shared.audio_offset_ms.load(Ordering::Relaxed)) * 10_000;
    let mut pcm = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
    while let Some(chunk) = audio.take(Duration::ZERO) {
        let start = (chunk
            .start
            .saturating_duration_since(shared.epoch)
            .as_nanos()
            / 100) as i64;
        pcm.push(start + offset, &chunk.samples);
    }
}

/// Encodes `frame`, repacking its rows first when the stride is padded.
pub(super) fn encode_frame(
    encoder: &mut H264Encoder,
    frame: &Frame,
    timestamp: i64,
    scratch: &mut Vec<u8>,
) -> windows::core::Result<Vec<EncodedFrame>> {
    if frame.stride == frame.width {
        return encoder.encode(&frame.data, timestamp);
    }
    let (w, h, stride) = (
        frame.width as usize,
        frame.height as usize,
        frame.stride as usize,
    );
    scratch.clear();
    for row in 0..h {
        scratch.extend_from_slice(&frame.data[row * stride..row * stride + w]);
    }
    let uv = stride * h;
    for row in 0..h / 2 {
        scratch.extend_from_slice(&frame.data[uv + row * stride..uv + row * stride + w]);
    }
    encoder.encode(scratch, timestamp)
}
