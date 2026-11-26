//! One running copy of a program, and passing it a request from a later
//! copy: open its window, close it, or exit.
//!
//! On Windows this is a named mutex plus one named event per request, all in
//! the user's session (`Local\<name>-instance`, `-show`, `-hide`, `-exit`).
//! The first copy holds the mutex and a thread of its own waits on the
//! events; a later copy finds the mutex, signals one event and ends. The
//! caller picks the name, so two builds (or two test homes) never meet.
//! Elsewhere every copy is the first one: the host lock still keeps a second
//! host from starting.

use std::time::Duration;

/// What a later copy asks of the running one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Open the window (a plain second start).
    Show,
    /// Close the window (`--hide`).
    Hide,
    /// Exit (`--exit`).
    Exit,
}

/// Held by the running copy for as long as it runs.
pub struct Instance {
    _inner: imp::Inner,
}

impl Instance {
    /// `Some` for the first copy, which then hears every request through
    /// `on_request` (on a thread of its own). `None` when another copy holds
    /// the name.
    pub fn acquire(name: &str, on_request: impl Fn(Request) + Send + 'static) -> Option<Instance> {
        imp::acquire(name, Box::new(on_request)).map(|inner| Instance { _inner: inner })
    }
}

/// Whether a copy holds the name now.
pub fn is_running(name: &str) -> bool {
    imp::is_running(name)
}

/// Passes a request to the running copy. Returns whether one runs; for
/// [`Request::Exit`] it then waits up to `wait` for that copy to end, and
/// returns false if it is still there.
pub fn send(name: &str, request: Request, wait: Duration) -> bool {
    imp::send(name, request, wait)
}

#[cfg(not(windows))]
mod imp {
    use super::Request;
    use std::time::Duration;

    pub struct Inner;

    pub fn acquire(_name: &str, _on_request: Box<dyn Fn(Request) + Send>) -> Option<Inner> {
        Some(Inner)
    }

    pub fn is_running(_name: &str) -> bool {
        false
    }

    pub fn send(_name: &str, _request: Request, _wait: Duration) -> bool {
        false
    }
}

#[cfg(windows)]
mod imp {
    pub use crate::win::single::{Inner, acquire, is_running, send};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique(tag: &str) -> String {
        format!("N8TrayTest-{tag}-{}", std::process::id())
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_every_copy_is_the_first() {
        let name = unique("other");
        let first = Instance::acquire(&name, |_| {});
        assert!(first.is_some());
        assert!(Instance::acquire(&name, |_| {}).is_some());
        assert!(!is_running(&name));
        assert!(!send(&name, Request::Show, Duration::ZERO));
    }

    #[cfg(windows)]
    #[test]
    fn a_later_copy_reaches_the_first() {
        use std::sync::mpsc;
        let name = unique("reach");
        assert!(!is_running(&name));
        assert!(!send(&name, Request::Show, Duration::ZERO));
        let (tx, rx) = mpsc::channel();
        let first = Instance::acquire(&name, move |r| {
            let _ = tx.send(r);
        })
        .expect("the first copy");
        assert!(is_running(&name));
        assert!(Instance::acquire(&name, |_| {}).is_none());
        let wait = || rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(send(&name, Request::Show, Duration::ZERO));
        assert_eq!(wait(), Request::Show);
        assert!(send(&name, Request::Hide, Duration::ZERO));
        assert_eq!(wait(), Request::Hide);
        drop(first);
        assert!(!is_running(&name));
        // Released: a new first copy can take it.
        assert!(Instance::acquire(&name, |_| {}).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn exit_waits_for_the_copy_to_end() {
        use std::sync::{Arc, Mutex};
        let name = unique("exit");
        let slot: Arc<Mutex<Option<Instance>>> = Arc::default();
        let held = Arc::clone(&slot);
        let instance = Instance::acquire(&name, move |r| {
            if r == Request::Exit {
                drop(held.lock().unwrap().take());
            }
        });
        *slot.lock().unwrap() = instance;
        assert!(send(&name, Request::Exit, Duration::from_secs(5)));
        assert!(!is_running(&name));
    }

    #[cfg(windows)]
    #[test]
    fn exit_gives_up_on_a_copy_that_stays() {
        let name = unique("stays");
        let _first = Instance::acquire(&name, |_| {}).unwrap();
        let started = std::time::Instant::now();
        assert!(!send(&name, Request::Exit, Duration::from_millis(300)));
        assert!(started.elapsed() >= Duration::from_millis(300));
    }

    #[cfg(windows)]
    #[test]
    fn names_do_not_meet() {
        let (a, b) = (unique("a"), unique("b"));
        let _first = Instance::acquire(&a, |_| {}).unwrap();
        assert!(!is_running(&b));
        assert!(Instance::acquire(&b, |_| {}).is_some());
    }
}
