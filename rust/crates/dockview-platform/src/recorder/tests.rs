use super::*;
use crate::test_support::hardware_or_skip;
use crate::testsrc::TestSource;

/// Records a synthetic source; skips quietly on a PC without a hardware
/// H.264 encoder, which is not a failure of the code under test.
fn record(seconds: f64) -> Option<(ReplayRecorder, TestSource)> {
    if !hardware_or_skip("a hardware H.264 encoder") {
        return None;
    }
    let slot = Arc::new(FrameSlot::new());
    let source = TestSource::start(Arc::clone(&slot), 640, 360, 30);
    let recorder = ReplayRecorder::start(
        slot,
        ReplaySettings {
            keep_seconds: 10.0,
            bitrate_mbps: 4,
            fps: 30,
        },
    );
    thread::sleep(Duration::from_secs_f64(seconds));
    match recorder.status() {
        RecorderStatus::Unsupported(why) => {
            eprintln!("skipped: {why}");
            None
        }
        _ => Some((recorder, source)),
    }
}

#[test]
fn it_records_and_saves_a_clip_that_is_longer_than_zero() {
    let Some((recorder, _source)) = record(3.5) else {
        return;
    };
    assert_eq!(recorder.status(), RecorderStatus::Recording);
    assert!(
        recorder.buffered_seconds() > 2.0,
        "{}",
        recorder.buffered_seconds()
    );
    assert!(recorder.buffered_bytes() > 0);

    let path = std::env::temp_dir().join(format!("dockview-rec-{}.mp4", std::process::id()));
    let saved = recorder
        .save(2.0, path.clone())
        .wait(Duration::from_secs(20))
        .expect("save finished")
        .expect("save succeeded");
    let _ = std::fs::remove_file(&path);
    assert!(saved.seconds >= 2.0, "{saved:?}");
    assert!(saved.bytes > 1_000 && saved.frames >= 55, "{saved:?}");
}

/// Decodes the first audio stream of an MP4: (seconds, RMS level 0..1).
fn audio_summary(path: &Path) -> Option<(f64, f64)> {
    use windows::Win32::Media::MediaFoundation::MF_SOURCE_READER_FIRST_AUDIO_STREAM;
    audio_track_summary(path, MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
}

/// Length in seconds and RMS level of the file's stream number `stream`
/// (0 is the video; audio tracks follow in file order).
fn audio_track_summary(path: &Path, stream: u32) -> Option<(f64, f64)> {
    use windows::core::HSTRING;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFSample, MFAudioFormat_PCM, MFCreateMediaType,
        MFCreateSourceReaderFromURL, MFMediaType_Audio, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
        MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
        MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
    };
    let _session = MfSession::start().ok()?;
    // SAFETY: plain source-reader calls on a file this test just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)
                .ok()?;
        let pcm = MFCreateMediaType().ok()?;
        pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).ok()?;
        pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).ok()?;
        pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).ok()?;
        pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)
            .ok()?;
        pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).ok()?;
        pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4).ok()?;
        pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)
            .ok()?;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .ok()?;
        reader.SetStreamSelection(stream, true).ok()?;
        reader.SetCurrentMediaType(stream, None, &pcm).ok()?;

        let (mut frames, mut sum_squares) = (0u64, 0f64);
        loop {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut ts),
                    Some(&mut sample),
                )
                .ok()?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            let buffer = sample.ConvertToContiguousBuffer().ok()?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut data, None, Some(&mut len)).ok()?;
            let bytes = std::slice::from_raw_parts(data, len as usize);
            let (pairs, _) = bytes.as_chunks::<2>();
            for pair in pairs {
                let v = f64::from(i16::from_le_bytes(*pair)) / 32768.0;
                sum_squares += v * v;
            }
            frames += (len / 4) as u64;
            buffer.Unlock().ok()?;
        }
        (frames > 0).then(|| {
            (
                frames as f64 / 48_000.0,
                (sum_squares / (frames as f64 * 2.0)).sqrt(),
            )
        })
    }
}

#[test]
fn a_saved_clip_carries_the_recorded_audio_at_the_right_length_and_level() {
    let Some((recorder, _source)) = record(0.2) else {
        return;
    };
    // Feed a 440 Hz sine at half scale, in 10 ms blocks, in real time.
    let tap = recorder.audio_tap();
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = {
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut phase = 0.0f32;
            while !stop.load(Ordering::Relaxed) {
                let mut block = Vec::with_capacity(960);
                for _ in 0..480 {
                    let v = 0.5 * (phase * std::f32::consts::TAU).sin();
                    block.extend_from_slice(&[v, v]);
                    phase = (phase + 440.0 / 48_000.0).fract();
                }
                tap.offer(Instant::now(), &block);
                thread::sleep(Duration::from_millis(10));
            }
        })
    };
    thread::sleep(Duration::from_secs(4));
    assert!(
        recorder.buffered_audio_seconds() > 2.5,
        "{}",
        recorder.buffered_audio_seconds()
    );

    let path = std::env::temp_dir().join(format!("dockview-rec-av-{}.mp4", std::process::id()));
    let saved = recorder
        .save(2.0, path.clone())
        .wait(Duration::from_secs(20))
        .expect("save finished")
        .expect("save succeeded");
    stop.store(true, Ordering::Relaxed);
    let _ = feeder.join();

    let summary = audio_summary(&path);
    let _ = std::fs::remove_file(&path);
    let (seconds, rms) = summary.expect("the clip has a decodable audio track");
    assert!(
        (seconds - saved.seconds).abs() < 0.25,
        "audio {seconds:.2}s vs video {:.2}s",
        saved.seconds
    );
    // A sine of amplitude 0.5 has an RMS of about 0.354; AAC is lossy.
    assert!((0.28..0.45).contains(&rms), "RMS {rms:.3}");
}

/// Feeds a sine of the given amplitude and pitch into `tap` in real time.
fn feed_sine(
    tap: Arc<AudioTap>,
    amplitude: f32,
    hz: f32,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut phase = 0.0f32;
        while !stop.load(Ordering::Relaxed) {
            let mut block = Vec::with_capacity(960);
            for _ in 0..480 {
                let v = amplitude * (phase * std::f32::consts::TAU).sin();
                block.extend_from_slice(&[v, v]);
                phase = (phase + hz / 48_000.0).fract();
            }
            tap.offer(Instant::now(), &block);
            thread::sleep(Duration::from_millis(10));
        }
    })
}

#[test]
fn a_clip_with_a_microphone_has_mix_game_and_mic_tracks() {
    let Some((recorder, _source)) = record(0.2) else {
        return;
    };
    let stop = Arc::new(AtomicBool::new(false));
    let feeders = [
        feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop)),
        feed_sine(recorder.mic_tap(), 0.25, 880.0, Arc::clone(&stop)),
    ];
    thread::sleep(Duration::from_secs(4));

    let path = std::env::temp_dir().join(format!("dockview-rec-mic-{}.mp4", std::process::id()));
    let saved = recorder
        .save(2.0, path.clone())
        .wait(Duration::from_secs(20))
        .expect("save finished")
        .expect("save succeeded");
    let tracks: Vec<_> = (0..=4).map(|n| audio_track_summary(&path, n)).collect();
    let fourth = audio_track_summary(&path, 3); // the video, not audio

    // Forgetting the microphone drops its track from the next clip.
    recorder.clear_mic();
    stop.store(true, Ordering::Relaxed);
    for f in feeders {
        let _ = f.join();
    }
    recorder.clear_mic();
    let path2 = std::env::temp_dir().join(format!("dockview-rec-nomic-{}.mp4", std::process::id()));
    recorder
        .save(2.0, path2.clone())
        .wait(Duration::from_secs(20))
        .expect("save finished")
        .expect("save succeeded");
    let second = audio_track_summary(&path2, 2);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);

    // Media Foundation's reader lists the audio tracks last-added first (the
    // file itself is video, mix, game, mic) and the video after them.
    let [mic, game, mix] = [tracks[0], tracks[1], tracks[2]].map(|t| t.expect("track decodes"));
    for (name, (seconds, _)) in [("mix", mix), ("game", game), ("mic", mic)] {
        assert!(
            (seconds - saved.seconds).abs() < 0.25,
            "{name} {seconds:.2}s vs video {:.2}s",
            saved.seconds
        );
    }
    // Sines of 0.5 and 0.25 have RMS 0.354 and 0.177; the mix is their power sum.
    assert!((0.33..0.46).contains(&mix.1), "mix RMS {:.3}", mix.1);
    assert!((0.28..0.45).contains(&game.1), "game RMS {:.3}", game.1);
    assert!((0.13..0.23).contains(&mic.1), "mic RMS {:.3}", mic.1);
    assert!(fourth.is_none(), "exactly three audio tracks");
    assert!(second.is_none(), "no microphone, no second audio track");
}

fn wait_for_outcome(recorder: &ReplayRecorder) -> RecordingOutcome {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if let Some(outcome) = recorder.take_recording_result() {
            return outcome;
        }
        assert!(Instant::now() < deadline, "the recording never finished");
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_recording_result(recorder: &ReplayRecorder) -> Result<SavedClip, String> {
    wait_for_outcome(recorder).result
}

/// A recorder fed by a test source, keeping the slot so the source can be
/// swapped for another (to simulate a signal that drops or changes size).
fn recorder_with_slot() -> Option<(ReplayRecorder, Arc<FrameSlot>, TestSource)> {
    if !hardware_or_skip("a hardware H.264 encoder") {
        return None;
    }
    let slot = Arc::new(FrameSlot::new());
    let source = TestSource::start(Arc::clone(&slot), 640, 360, 30);
    let recorder = ReplayRecorder::start(
        Arc::clone(&slot),
        ReplaySettings {
            keep_seconds: 10.0,
            bitrate_mbps: 4,
            fps: 30,
        },
    );
    let ready = Instant::now() + Duration::from_secs(5);
    while recorder.status() == RecorderStatus::Starting && Instant::now() < ready {
        thread::sleep(Duration::from_millis(20));
    }
    match recorder.status() {
        RecorderStatus::Recording => Some((recorder, slot, source)),
        other => {
            eprintln!("skipped: {other:?}");
            None
        }
    }
}

/// Number of video samples a file holds and where the last one ends.
fn video_samples(path: &Path) -> Option<(u64, f64)> {
    use windows::core::HSTRING;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFSample, MFCreateSourceReaderFromURL, MF_SOURCE_READERF_ENDOFSTREAM,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    };
    let _session = MfSession::start().ok()?;
    // SAFETY: plain source-reader calls on a file this test just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)
                .ok()?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let (mut count, mut end) = (0u64, 0i64);
        loop {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut ts),
                    Some(&mut sample),
                )
                .ok()?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if let Some(sample) = sample {
                count += 1;
                end = ts + sample.GetSampleDuration().unwrap_or(0);
            }
        }
        Some((count, end as f64 / 1e7))
    }
}

#[test]
fn a_resolution_change_closes_the_file_and_goes_on_in_a_new_one() {
    let Some((recorder, slot, source)) = recorder_with_slot() else {
        return;
    };
    let path = std::env::temp_dir().join(format!("dockview-parts-{}.mp4", std::process::id()));
    let second_path = part_path(&path, 2);
    recorder.start_recording(path.clone()).expect("starts");
    thread::sleep(Duration::from_secs(6));
    drop(source);
    let _bigger = TestSource::start(Arc::clone(&slot), 800, 448, 30);
    thread::sleep(Duration::from_secs(6));
    recorder.stop_recording();

    let first = wait_for_outcome(&recorder);
    let second = wait_for_outcome(&recorder);
    let first_video = first
        .result
        .as_ref()
        .ok()
        .and_then(|c| video_samples(&c.path));
    let second_video = second
        .result
        .as_ref()
        .ok()
        .and_then(|c| video_samples(&c.path));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&second_path);

    assert_eq!(first.reason, EndReason::ResolutionChanged);
    assert!(first.continues, "recording goes on in a second file");
    let first_clip = first.result.expect("the first file was saved");
    assert!(first_clip.seconds > 1.0, "{}", first_clip.seconds);
    assert_eq!(first_clip.path, path);

    assert_eq!(second.reason, EndReason::Stopped);
    assert!(!second.continues);
    let second_clip = second.result.expect("the second file was saved");
    assert!(second_clip.seconds > 1.0, "{}", second_clip.seconds);
    assert_eq!(second_clip.path, second_path);

    assert!(first_video.is_some_and(|(n, _)| n > 30), "{first_video:?}");
    assert!(
        second_video.is_some_and(|(n, _)| n > 30),
        "{second_video:?}"
    );
    assert_eq!(recorder.recording().state, RecordingState::Idle);
}

#[test]
fn a_signal_that_drops_and_returns_stays_one_valid_file() {
    let Some((recorder, slot, source)) = recorder_with_slot() else {
        return;
    };
    let path = std::env::temp_dir().join(format!("dockview-gap-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    thread::sleep(Duration::from_secs(4));
    drop(source); // the capture card goes away...
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        recorder.recording().state,
        RecordingState::Recording,
        "a short outage does not end the recording"
    );
    let _back = TestSource::start(Arc::clone(&slot), 640, 360, 30); // ...and returns
    thread::sleep(Duration::from_secs(4));
    recorder.stop_recording();

    let outcome = wait_for_outcome(&recorder);
    let clip = outcome.result.expect("saved");
    let video = video_samples(&path);
    let _ = std::fs::remove_file(&path);

    assert_eq!(outcome.reason, EndReason::Stopped);
    // About 4 s + the 3 s outage + 4 s (less the wait for the first keyframe):
    // the outage is part of the timeline.
    assert!((6.5..12.0).contains(&clip.seconds), "{}", clip.seconds);
    let (samples, end) = video.expect("the file reads");
    assert!(samples >= 100, "{samples} samples");
    assert!(
        (end - clip.seconds).abs() < 0.5,
        "{end} vs {}",
        clip.seconds
    );
}

#[test]
fn asking_to_record_without_video_gives_up_with_no_signal() {
    let Some((recorder, _slot, source)) = recorder_with_slot() else {
        return;
    };
    drop(source); // no video from now on
    recorder.set_no_signal_timeout(Duration::from_secs(1));
    let path = std::env::temp_dir().join(format!("dockview-nosig-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    let outcome = wait_for_outcome(&recorder);
    let _ = std::fs::remove_file(&path);
    assert_eq!(outcome.reason, EndReason::NoSignal);
    assert!(outcome.result.is_err());
    assert!(!outcome.continues);
    assert_eq!(recorder.recording().state, RecordingState::Idle);
}

#[test]
fn a_recording_stops_cleanly_when_the_disk_is_about_to_fill() {
    let Some((recorder, _slot, _source)) = recorder_with_slot() else {
        return;
    };
    let path = std::env::temp_dir().join(format!("dockview-full-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    thread::sleep(Duration::from_secs(2));
    // Pretend the disk needs more free space than it has.
    recorder.set_disk_stop_bytes(u64::MAX);
    let outcome = wait_for_outcome(&recorder);
    let video = video_samples(&path);
    let _ = std::fs::remove_file(&path);

    assert_eq!(outcome.reason, EndReason::DiskFull);
    let clip = outcome.result.expect("what was recorded is saved");
    assert!(clip.seconds > 1.0, "{}", clip.seconds);
    assert!(video.is_some_and(|(n, _)| n > 30), "{video:?}");
    assert_eq!(recorder.recording().state, RecordingState::Idle);
}

#[test]
fn a_recording_is_a_fragmented_file_with_video_and_three_audio_tracks() {
    let Some((recorder, _source)) = record(0.2) else {
        return;
    };
    let stop = Arc::new(AtomicBool::new(false));
    let feeders = [
        feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop)),
        feed_sine(recorder.mic_tap(), 0.25, 880.0, Arc::clone(&stop)),
    ];
    thread::sleep(Duration::from_secs(2)); // let audio reach the rings first

    let path = std::env::temp_dir().join(format!("dockview-recording-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    assert!(
        recorder.start_recording(path.clone()).is_err(),
        "a second start is refused"
    );
    thread::sleep(Duration::from_secs(5));
    let info = recorder.recording();
    assert_eq!(info.state, RecordingState::Recording);
    assert!(info.seconds > 2.5, "{info:?}");
    assert!(info.bytes > 0, "the file grows while recording");

    recorder.stop_recording();
    let saved = wait_for_recording_result(&recorder).expect("the recording was saved");
    stop.store(true, Ordering::Relaxed);
    for f in feeders {
        let _ = f.join();
    }
    assert_eq!(recorder.recording().state, RecordingState::Idle);

    let bytes = std::fs::read(&path).expect("the file exists");
    let fragmented = bytes.windows(4).any(|w| w == b"moof");
    let tracks: Vec<_> = (0..3).map(|n| audio_track_summary(&path, n)).collect();
    let _ = std::fs::remove_file(&path);

    assert!(fragmented, "the file must be a fragmented MP4");
    // About 5 s are recorded; the lower bound is loose because a busy machine
    // (the whole suite running in parallel) drops a few frames.
    assert!((3.0..7.5).contains(&saved.seconds), "{}", saved.seconds);
    // The reader lists the tracks last-added first: mic, game, mix.
    let [mic, game, mix] = [tracks[0], tracks[1], tracks[2]].map(|t| t.expect("track decodes"));
    for (name, (seconds, _)) in [("mix", mix), ("game", game), ("mic", mic)] {
        assert!(
            (seconds - saved.seconds).abs() < 0.3,
            "{name} {seconds:.2}s vs video {:.2}s",
            saved.seconds
        );
    }
    assert!((0.33..0.46).contains(&mix.1), "mix RMS {:.3}", mix.1);
    assert!((0.28..0.45).contains(&game.1), "game RMS {:.3}", game.1);
    assert!((0.13..0.23).contains(&mic.1), "mic RMS {:.3}", mic.1);
}

#[test]
fn a_recording_cut_off_halfway_still_plays() {
    let Some((recorder, _source)) = record(0.2) else {
        return;
    };
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop));
    thread::sleep(Duration::from_secs(2));

    let path = std::env::temp_dir().join(format!("dockview-cutoff-{}.mp4", std::process::id()));
    let copy =
        std::env::temp_dir().join(format!("dockview-cutoff-copy-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    thread::sleep(Duration::from_secs(6));
    // What is on disk right now is what a crash would leave behind.
    std::fs::copy(&path, &copy).expect("copies the unfinished file");
    recorder.stop_recording();
    let _ = wait_for_recording_result(&recorder);
    stop.store(true, Ordering::Relaxed);
    let _ = feeder.join();

    let left_behind = audio_track_summary(&copy, 0);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&copy);
    let (seconds, _) = left_behind.expect("the unfinished file still opens and decodes");
    assert!(seconds > 1.5, "only {seconds:.2}s survived of about 6");
}

#[test]
fn a_recording_asked_for_but_stopped_at_once_reports_an_error() {
    let Some((recorder, _source)) = record(0.2) else {
        return;
    };
    let ready = Instant::now() + Duration::from_secs(5);
    while recorder.status() != RecorderStatus::Recording && Instant::now() < ready {
        thread::sleep(Duration::from_millis(20));
    }
    let path = std::env::temp_dir().join(format!("dockview-never-{}.mp4", std::process::id()));
    recorder.start_recording(path.clone()).expect("starts");
    recorder.stop_recording();
    // Either it never began (a quiet cancel) or a keyframe was so close
    // that it did.
    let outcome = wait_for_outcome(&recorder);
    let _ = std::fs::remove_file(&path);
    if let Err(e) = &outcome.result {
        assert!(e.contains("before the first frame"), "{e}");
        assert_eq!(outcome.reason, EndReason::Cancelled);
    }
    assert_eq!(recorder.recording().state, RecordingState::Idle);
}

#[test]
fn tracks_are_the_mix_then_game_then_mic_when_there_is_a_microphone() {
    let game = vec![100i16, 200];
    let mic = vec![1i16, 2];
    let tracks = audio_tracks(Some(game.clone()), Some(mic.clone()));
    assert_eq!(tracks, vec![vec![101, 202], game.clone(), mic.clone()]);
    assert_eq!(audio_tracks(Some(game.clone()), None), vec![game]);
    assert_eq!(audio_tracks(None, Some(mic.clone())), vec![mic]);
    assert!(audio_tracks(None, None).is_empty());
}

#[test]
fn a_clip_without_audio_is_still_valid() {
    let Some((recorder, _source)) = record(3.0) else {
        return;
    };
    let path = std::env::temp_dir().join(format!("dockview-rec-v-{}.mp4", std::process::id()));
    let saved = recorder
        .save(2.0, path.clone())
        .wait(Duration::from_secs(20))
        .unwrap()
        .unwrap();
    let has_audio = audio_summary(&path).is_some();
    let _ = std::fs::remove_file(&path);
    assert!(saved.frames > 0);
    assert!(!has_audio, "no audio was fed, so there is no audio track");
}

#[test]
fn saving_before_anything_is_recorded_reports_it() {
    let slot = Arc::new(FrameSlot::new());
    let recorder = ReplayRecorder::start(slot, ReplaySettings::default());
    let result = recorder
        .save(5.0, std::env::temp_dir().join("dockview-never.mp4"))
        .wait(Duration::from_secs(5))
        .expect("finished");
    assert_eq!(result.unwrap_err(), "nothing has been recorded yet");
}

#[test]
fn dropping_the_recorder_detaches_it_from_the_frame_stream() {
    let slot = Arc::new(FrameSlot::new());
    let recorder = ReplayRecorder::start(Arc::clone(&slot), ReplaySettings::default());
    drop(recorder);
    // Publishing afterwards must not panic or queue anywhere.
    slot.publish(crate::testsrc::nv12_bars(64, 36));
    assert_eq!(slot.published(), 1);
}
