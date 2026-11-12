//! Keeps the app from reloading its data more often than it needs to: bursts of file changes become one
//! reload, and a reload asked for while another runs becomes one more reload when that one ends.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// Whether a data load is running and whether another was asked for meanwhile.
#[derive(Debug, Default)]
pub struct Coalesce {
    running: bool,
    again: bool,
}

impl Coalesce {
    /// A load that is already running when the app starts.
    pub fn running() -> Self {
        Self {
            running: true,
            again: false,
        }
    }

    /// A reload was asked for. `true`: start it now. `false`: one is running, so it is remembered and
    /// will start when that one ends (the running one may have read the data before the change).
    pub fn request(&mut self) -> bool {
        if self.running {
            self.again = true;
            false
        } else {
            self.running = true;
            true
        }
    }

    /// The running load ended. `true`: another was asked for meanwhile, start it now.
    pub fn finished(&mut self) -> bool {
        if std::mem::take(&mut self.again) {
            self.running = true;
            true
        } else {
            self.running = false;
            false
        }
    }
}

/// Turns the bursts of `rx` into one `tx` message each: after the first event it waits until `quiet` passes
/// without another, but never longer than `longest`. Returns when either side is gone.
pub fn debounce(rx: &Receiver<()>, tx: &Sender<()>, quiet: Duration, longest: Duration) {
    while rx.recv().is_ok() {
        let started = Instant::now();
        loop {
            let left = longest.saturating_sub(started.elapsed()).min(quiet);
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = tx.send(());
                    return;
                }
            }
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn a_reload_asked_for_during_a_load_runs_once_when_it_ends() {
        let mut c = Coalesce::running();
        assert!(!c.request());
        assert!(!c.request(), "any number of requests is one more load");
        assert!(c.finished());
        assert!(!c.finished(), "and then it is idle");
        assert!(c.request(), "idle: a request starts right away");
        assert!(!c.request());
        assert!(c.finished());
        assert!(!c.finished());
    }

    #[test]
    fn a_burst_of_events_becomes_one_and_a_quiet_one_goes_through() {
        let (raw_tx, raw_rx) = channel();
        let (out_tx, out_rx) = channel();
        let worker = std::thread::spawn(move || {
            debounce(
                &raw_rx,
                &out_tx,
                Duration::from_millis(60),
                Duration::from_millis(500),
            )
        });
        for _ in 0..10 {
            raw_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(out_rx.recv_timeout(Duration::from_millis(400)).is_ok());
        assert!(
            out_rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "ten events, one message"
        );
        raw_tx.send(()).unwrap();
        assert!(out_rx.recv_timeout(Duration::from_millis(400)).is_ok());
        drop(raw_tx);
        worker.join().unwrap();
    }

    #[test]
    fn a_steady_stream_is_still_reported_within_the_longest_wait() {
        let (raw_tx, raw_rx) = channel();
        let (out_tx, out_rx) = channel();
        let sender = std::thread::spawn(move || {
            for _ in 0..40 {
                if raw_tx.send(()).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let worker = std::thread::spawn(move || {
            debounce(
                &raw_rx,
                &out_tx,
                Duration::from_millis(100),
                Duration::from_millis(150),
            )
        });
        // events never stop for 100 ms, yet the first message arrives by ~150 ms
        assert!(out_rx.recv_timeout(Duration::from_millis(300)).is_ok());
        sender.join().unwrap();
        drop(out_rx);
        let _ = worker.join();
    }
}
