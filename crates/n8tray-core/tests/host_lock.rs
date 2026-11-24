//! The host lock (`host.lock`), in a test binary of its own.
//!
//! The lock is an `flock`, which belongs to the open file, not to the
//! process: a child started while the lock is held shares the descriptor
//! until it execs (the descriptor is close-on-exec), and the lock lives that
//! long after the holder closed its own copy. In the library's test binary
//! other tests start processes in parallel (the interpreter tests start
//! `node`), and on the macOS runner "dropped, so free again" failed in about
//! 1 run in 25 of that binary, never when the test ran alone and never with
//! the `node` tests skipped. Here nothing else runs in the process.
//!
//! The tray never releases the lock while it runs (it is held across a
//! restart of the host thread and freed when the process exits), so the race
//! is the test's, not the product's.

use n8tray_core::lease::HostLock;

#[test]
fn one_host_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let first = HostLock::acquire(dir.path()).unwrap();
    assert!(first.is_some());
    assert!(HostLock::acquire(dir.path()).unwrap().is_none());
    drop(first);
    assert!(
        HostLock::acquire(dir.path()).unwrap().is_some(),
        "free again once the holder is gone"
    );
}
