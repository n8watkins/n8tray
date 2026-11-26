//! One running copy: a named mutex and one named event per request (see
//! `crate::single`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, OpenMutexW, SetEvent, WaitForMultipleObjects,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

use super::{Handle, wide};
use crate::single::Request;

const SYNCHRONIZE: u32 = 0x0010_0000;
/// The requests in the order of their events.
const REQUESTS: [Request; 3] = [Request::Show, Request::Hide, Request::Exit];

fn mutex_name(name: &str) -> Vec<u16> {
    wide(&format!("Local\\{name}-instance"))
}

fn event_name(name: &str, request: Request) -> Vec<u16> {
    let what = match request {
        Request::Show => "show",
        Request::Hide => "hide",
        Request::Exit => "exit",
    };
    wide(&format!("Local\\{name}-{what}"))
}

/// Creates (or opens) a named auto-reset event that starts unsignalled.
fn event(name: &[u16]) -> Option<Handle> {
    Handle::new(unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) })
}

pub struct Inner {
    // Field order is drop order: the mutex goes first, so the copy counts as
    // gone at once, even while the waiter is still stopping.
    _mutex: Handle,
    stop: Option<Arc<Handle>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            unsafe { SetEvent(stop.raw()) };
        }
        // `Exit` may drop the instance from the waiter thread itself.
        if let Some(join) = self.join.take()
            && join.thread().id() != std::thread::current().id()
        {
            let _ = join.join();
        }
    }
}

pub fn acquire(name: &str, on_request: Box<dyn Fn(Request) + Send>) -> Option<Inner> {
    let mutex_name = mutex_name(name);
    let raw = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    // Without a mutex nothing can tell; run as the first copy rather than
    // refuse to start (the host lock still keeps a second host out).
    let mutex = Handle::new(raw)?;
    if already {
        return None;
    }
    let events: Option<Vec<Handle>> = REQUESTS
        .iter()
        .map(|r| event(&event_name(name, *r)))
        .collect();
    // A manual-reset event with no name, to stop the waiter.
    let stop = Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) });
    let (Some(events), Some(stop)) = (events, stop) else {
        return Some(Inner {
            _mutex: mutex,
            stop: None,
            join: None,
        });
    };
    let stop = Arc::new(stop);
    let waiter_stop = Arc::clone(&stop);
    let join = std::thread::Builder::new()
        .name("n8tray-single".into())
        .spawn(move || {
            let mut handles: Vec<_> = events.iter().map(Handle::raw).collect();
            handles.push(waiter_stop.raw());
            loop {
                let r = unsafe {
                    WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE)
                };
                let index = r.wrapping_sub(WAIT_OBJECT_0) as usize;
                match REQUESTS.get(index) {
                    Some(request) => on_request(*request),
                    None => break,
                }
            }
            drop(events);
        })
        .ok();
    Some(Inner {
        _mutex: mutex,
        stop: Some(stop),
        join,
    })
}

pub fn is_running(name: &str) -> bool {
    let name = mutex_name(name);
    Handle::new(unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) }).is_some()
}

pub fn send(name: &str, request: Request, wait: Duration) -> bool {
    if !is_running(name) {
        return false;
    }
    // The user just started this copy, so it may hand the foreground on: the
    // running copy's window then opens in front, where Escape reaches it.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    if let Some(event) = event(&event_name(name, request)) {
        unsafe { SetEvent(event.raw()) };
        if request != Request::Exit {
            return true;
        }
        // Held open until the copy has seen it (an auto-reset event whose
        // last handle closes is gone, signalled or not).
        let end = Instant::now() + wait;
        while is_running(name) {
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(event);
        return true;
    }
    false
}
