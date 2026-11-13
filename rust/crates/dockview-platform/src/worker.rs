//! Background threads that stop cleanly. Every source, the compositor, the
//! recorder and the audio engine used to carry their own copy of "start a
//! thread, keep a stop flag, wait for it when dropped"; this is that, once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often the drop checks whether a thread has finished.
const POLL: Duration = Duration::from_millis(5);

/// One or more threads that share a stop flag. Dropping the worker raises the
/// flag and waits, up to a limit, for the threads to end. One that is still
/// blocked in a driver call when the time is up is left to finish on its own.
pub struct Worker {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    timeout: Duration,
}

impl Worker {
    /// A worker with no threads yet; `timeout` is how long a drop may wait.
    pub fn new(timeout: Duration) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
            timeout,
        }
    }

    /// Starts `body` on a thread called `name`. It gets the stop flag and
    /// should return soon after it is raised. Returns whether the thread
    /// could be started.
    pub fn spawn(&mut self, name: &str, body: impl FnOnce(&AtomicBool) + Send + 'static) -> bool {
        let stop = Arc::clone(&self.stop);
        match thread::Builder::new()
            .name(name.into())
            .spawn(move || body(&stop))
        {
            Ok(handle) => {
                self.threads.push(handle);
                true
            }
            Err(_) => false,
        }
    }

    /// Raises the stop flag without waiting.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Whether the stop flag is raised.
    pub fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// How many threads were started.
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.request_stop();
        let deadline = Instant::now() + self.timeout;
        for thread in self.threads.drain(..) {
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
            // Otherwise it is detached and ends on its own.
        }
    }
}

/// A value one thread keeps up to date and others read, such as a source's
/// status. A thread that panicked while holding it does not poison it.
#[derive(Debug)]
pub struct StatusCell<T> {
    value: Mutex<T>,
}

impl<T: Clone> StatusCell<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Mutex::new(value),
        }
    }

    pub fn set(&self, value: T) {
        *self.value.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }

    pub fn get(&self) -> T {
        self.value.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    const LONG: Duration = Duration::from_secs(2);

    #[test]
    fn dropping_raises_the_flag_and_waits_for_the_thread() {
        let ended = Arc::new(AtomicBool::new(false));
        let mut worker = Worker::new(LONG);
        let seen = Arc::clone(&ended);
        assert!(worker.spawn("test-worker", move |stop| {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
            seen.store(true, Ordering::SeqCst);
        }));
        assert_eq!(worker.thread_count(), 1);
        assert!(!worker.is_stopping());
        drop(worker);
        // The drop only returns after the thread finished.
        assert!(ended.load(Ordering::SeqCst));
    }

    #[test]
    fn all_threads_share_one_flag() {
        let ended = Arc::new(AtomicU32::new(0));
        let mut worker = Worker::new(LONG);
        for name in ["a", "b", "c"] {
            let ended = Arc::clone(&ended);
            worker.spawn(name, move |stop| {
                while !stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(1));
                }
                ended.fetch_add(1, Ordering::SeqCst);
            });
        }
        drop(worker);
        assert_eq!(ended.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_stuck_thread_does_not_hold_the_drop_past_the_limit() {
        let release = Arc::new(AtomicBool::new(false));
        let mut worker = Worker::new(Duration::from_millis(60));
        let gate = Arc::clone(&release);
        worker.spawn("stuck", move |_| {
            // Ignores the stop flag, like a thread blocked in a driver call.
            while !gate.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
        });
        let started = Instant::now();
        drop(worker);
        assert!(started.elapsed() < Duration::from_millis(900));
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn a_worker_without_threads_drops_at_once() {
        let started = Instant::now();
        drop(Worker::new(LONG));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn the_status_cell_survives_a_panic_in_a_writer() {
        let cell = Arc::new(StatusCell::new(1));
        let writer = Arc::clone(&cell);
        let _ = thread::spawn(move || {
            let _guard = writer.value.lock().unwrap();
            panic!("poisons the lock");
        })
        .join();
        cell.set(7);
        assert_eq!(cell.get(), 7);
    }
}
