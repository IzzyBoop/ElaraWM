//! Running the main thread's run loop.

use std::cell::Cell;
use std::ffi::c_void;
use std::time::Duration;

use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFAbsoluteTimeGetCurrent, CFRunLoop, CFRunLoopActivity,
    CFRunLoopObserver, CFRunLoopObserverContext, CFRunLoopTimer, CFRunLoopTimerContext,
};
use objc2_foundation::MainThreadMarker;

type Callback<'a> = &'a mut dyn FnMut() -> bool;

thread_local! {
    /// Set while the callback runs, so that it is never entered a second time if something it
    /// calls spins the run loop.
    static IN_CALLBACK: Cell<bool> = const { Cell::new(false) };
}

/// Calls the closure that `info` points to, and stops the run loop if it returns `false`.
///
/// # Safety
///
/// `info` must be the pointer `run()` put into the observer and timer contexts, and `run()`
/// must not have returned.
unsafe fn wake(info: *mut c_void) {
    if IN_CALLBACK.replace(true) {
        return;
    }
    // SAFETY: per this function's contract, `info` points to a live `Callback` on `run()`'s
    // stack, and the guard above means no other reference to it is active.
    let callback = unsafe { &mut *info.cast::<Callback<'_>>() };
    let keep_going = callback();
    IN_CALLBACK.set(false);

    if !keep_going {
        if let Some(run_loop) = CFRunLoop::main() {
            run_loop.stop();
        }
    }
}

unsafe extern "C-unwind" fn observer_fired(
    _observer: *mut CFRunLoopObserver,
    _activity: CFRunLoopActivity,
    info: *mut c_void,
) {
    // SAFETY: the run loop passes back the `info` from the context given in `run()`.
    unsafe { wake(info) }
}

unsafe extern "C-unwind" fn timer_fired(_timer: *mut CFRunLoopTimer, info: *mut c_void) {
    // SAFETY: as in `observer_fired()`.
    unsafe { wake(info) }
}

/// Runs the main run loop, calling `on_wake` each time the loop has handled what was pending
/// and is about to sleep, and at least once every `interval`.
///
/// This is what delivers a backend's events: Accessibility and workspace notifications arrive as
/// run loop sources, and `on_wake` runs right after them. Returns when `on_wake` returns
/// `false`.
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn run(interval: Duration, mut on_wake: impl FnMut() -> bool) {
    MainThreadMarker::new().expect("the run loop must be run on the main thread");
    let run_loop = CFRunLoop::main().expect("the main thread has a run loop");

    let mut callback: Callback<'_> = &mut on_wake;
    let info = (&raw mut callback).cast::<c_void>();

    let mut observer_context = CFRunLoopObserverContext {
        version: 0,
        info,
        retain: None,
        release: None,
        copyDescription: None,
    };
    let mut timer_context = CFRunLoopTimerContext {
        version: 0,
        info,
        retain: None,
        release: None,
        copyDescription: None,
    };

    // SAFETY: the callbacks have the signatures the API expects. The contexts are copied by the
    // constructors. `info` points to `callback`, which outlives the observer and the timer:
    // both are invalidated below before this function returns.
    let (observer, timer) = unsafe {
        let observer = CFRunLoopObserver::new(
            None,
            CFRunLoopActivity::BeforeWaiting.0,
            true,
            0,
            Some(observer_fired),
            &mut observer_context,
        )
        .expect("creating a run loop observer cannot fail");
        let timer = CFRunLoopTimer::new(
            None,
            CFAbsoluteTimeGetCurrent() + interval.as_secs_f64(),
            interval.as_secs_f64(),
            0,
            0,
            Some(timer_fired),
            &mut timer_context,
        )
        .expect("creating a run loop timer cannot fail");
        (observer, timer)
    };

    // SAFETY: the mode constant is a valid static.
    let mode = unsafe { kCFRunLoopDefaultMode };
    run_loop.add_observer(Some(&observer), mode);
    run_loop.add_timer(Some(&timer), mode);

    CFRunLoop::run();

    observer.invalidate();
    timer.invalidate();
}
