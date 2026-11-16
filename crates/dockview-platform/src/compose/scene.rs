//! The compositor thread: draws a new picture whenever a visible source has a
//! new frame, and publishes it.

use super::*;

/// Runs a [`Compositor`] on a thread of its own while it is alive, drawing a
/// new picture whenever a visible source has a new frame (or the layers
/// change).
pub struct SceneCompositor {
    layers: Arc<Mutex<(Vec<Layer>, bool)>>,
    status: Arc<StatusCell<SceneStatus>>,
    _worker: Worker,
}

/// How long dropping the compositor waits for its thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the thread waits for the bottom layer before it looks at the
/// others (the camera can be the only thing moving).
const POLL: Duration = Duration::from_millis(4);

/// The most pictures per second the compositor makes. Two sources of 60 fps
/// would otherwise add up to 120 pictures a second, which nobody can use.
pub const OUTPUT_FPS: u32 = 60;

impl SceneCompositor {
    pub fn start(layers: Vec<Layer>, out: Arc<FrameSlot>, width: u32, height: u32) -> Self {
        let layers = Arc::new(Mutex::new((layers, true)));
        let status = Arc::new(StatusCell::new(SceneStatus::Starting));
        let mut worker = Worker::new(STOP_TIMEOUT);
        let (thread_layers, thread_status) = (Arc::clone(&layers), Arc::clone(&status));
        let started = worker.spawn("scene-compositor", move |stop| {
            // SAFETY: plain timer-resolution requests, undone below.
            unsafe {
                timeBeginPeriod(1);
            }
            let result = run(&thread_layers, &out, width, height, stop, &thread_status);
            // SAFETY: matches the call above.
            unsafe {
                timeEndPeriod(1);
            }
            if let Err(e) = result {
                thread_status.set(SceneStatus::Failed(e.message()));
            }
        });
        if !started {
            status.set(SceneStatus::Failed(
                "could not start the compositor thread".into(),
            ));
        }
        Self {
            layers,
            status,
            _worker: worker,
        }
    }

    /// Replaces the layers (a scene change or an edit).
    pub fn set_layers(&self, layers: Vec<Layer>) {
        *self.layers.lock().unwrap_or_else(|e| e.into_inner()) = (layers, true);
    }

    pub fn status(&self) -> SceneStatus {
        self.status.get()
    }
}

fn run(
    layers: &Mutex<(Vec<Layer>, bool)>,
    out: &FrameSlot,
    width: u32,
    height: u32,
    stop: &AtomicBool,
    status: &StatusCell<SceneStatus>,
) -> windows::core::Result<()> {
    let mut compositor = Compositor::new(width, height)?;
    status.set(SceneStatus::Running);
    let started = Instant::now();
    let mut seen: Vec<u64> = Vec::new();
    let mut last_picture = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let (current, changed) = {
            let mut guard = layers.lock().unwrap_or_else(|e| e.into_inner());
            let changed = std::mem::take(&mut guard.1);
            (guard.0.clone(), changed)
        };
        // Wake on the first visible layer; the rest are polled.
        let wait_on = current.iter().find(|l| l.visible);
        let fresh = match wait_on {
            Some(layer) => layer.slot.wait_for_frame(POLL),
            None => {
                thread::sleep(POLL);
                false
            }
        };
        seen.resize(current.len(), 0);
        let mut news = fresh || changed;
        for (index, layer) in current.iter().enumerate() {
            let published = layer.slot.published();
            if layer.visible && published != seen[index] {
                news = true;
            }
            seen[index] = published;
        }
        if !news {
            continue;
        }
        // At most one picture per period; the newest frames win meanwhile.
        let period = Duration::from_secs_f64(1.0 / f64::from(OUTPUT_FPS));
        let due = last_picture + period;
        let now = Instant::now();
        if now < due {
            thread::sleep(due - now);
        }
        last_picture = Instant::now();
        let timestamp = (started.elapsed().as_nanos() / 100) as i64;
        compositor.compose(&current, out, timestamp)?;
    }
    Ok(())
}
