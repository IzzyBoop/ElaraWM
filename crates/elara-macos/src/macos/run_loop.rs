//! Running the main thread's run loop.

use std::cell::Cell;
use std::ffi::c_void;
use std::time::{Duration, Instant};

use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFAbsoluteTimeGetCurrent, CFRetained, CFRunLoop, CFRunLoopActivity,
    CFRunLoopObserver, CFRunLoopObserverContext, CFRunLoopTimer, CFRunLoopTimerContext,
};
use objc2_foundation::MainThreadMarker;

/// What the callback of [`run()`] wants next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// Leave the run loop.
    Stop,
    /// Nothing to do until something happens.
    WhenNeeded,
    /// Call again after this long even if nothing happens, as for the next frame of an
    /// animation.
    After(Duration),
}

/// A pass that ends less than this long before the loop goes back to sleep is not repeated.
const MIN_GAP_BEFORE_SLEEP: Duration = Duration::from_millis(2);

struct Context<'a> {
    callback: &'a mut dyn FnMut() -> Wake,
    timer: Option<CFRetained<CFRunLoopTimer>>,
    /// How often to call back when nothing happens.
    idle_interval: Duration,
    last_finished: Option<Instant>,
}

thread_local! {
    /// Set while the callback runs, so that it is never entered a second time if something it
    /// calls spins the run loop.
    static IN_CALLBACK: Cell<bool> = const { Cell::new(false) };
}

/// Calls the closure in the context that `info` points to and acts on what it returns.
///
/// # Safety
///
/// `info` must be the pointer `run()` put into the observer and timer contexts, and `run()`
/// must not have returned.
unsafe fn wake(info: *mut c_void, from_timer: bool) {
    if IN_CALLBACK.replace(true) {
        return;
    }
    // SAFETY: per this function's contract, `info` points to a live `Context` on `run()`'s
    // stack, and the guard above means no other reference to it is active.
    let context = unsafe { &mut *info.cast::<Context<'_>>() };

    // The loop is about to sleep right after a timer pass: there is nothing new to do, and
    // doing it again would move every animating window twice per frame.
    let just_ran = context
        .last_finished
        .is_some_and(|at| at.elapsed() < MIN_GAP_BEFORE_SLEEP);
    if !from_timer && just_ran {
        IN_CALLBACK.set(false);
        return;
    }

    let next = (context.callback)();
    context.last_finished = Some(Instant::now());
    IN_CALLBACK.set(false);

    let delay = match next {
        Wake::Stop => {
            if let Some(run_loop) = CFRunLoop::main() {
                run_loop.stop();
            }
            return;
        }
        Wake::WhenNeeded => context.idle_interval,
        Wake::After(delay) => delay.min(context.idle_interval),
    };
    if let Some(timer) = &context.timer {
        timer.set_next_fire_date(CFAbsoluteTimeGetCurrent() + delay.as_secs_f64());
    }
}

unsafe extern "C-unwind" fn observer_fired(
    _observer: *mut CFRunLoopObserver,
    _activity: CFRunLoopActivity,
    info: *mut c_void,
) {
    // SAFETY: the run loop passes back the `info` from the context given in `run()`.
    unsafe { wake(info, false) }
}

unsafe extern "C-unwind" fn timer_fired(_timer: *mut CFRunLoopTimer, info: *mut c_void) {
    // SAFETY: as in `observer_fired()`.
    unsafe { wake(info, true) }
}

/// Runs the main run loop, calling `on_wake` each time the loop has handled what was pending
/// and is about to sleep, and at least once every `idle_interval`.
///
/// This is what delivers a backend's events: Accessibility and workspace notifications arrive as
/// run loop sources, and `on_wake` runs right after them. `on_wake` says when it wants to run
/// next; returns when it says [`Wake::Stop`].
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn run(idle_interval: Duration, mut on_wake: impl FnMut() -> Wake) {
    MainThreadMarker::new().expect("the run loop must be run on the main thread");
    let run_loop = CFRunLoop::main().expect("the main thread has a run loop");

    let mut context = Context {
        callback: &mut on_wake,
        timer: None,
        idle_interval,
        last_finished: None,
    };
    let info = (&raw mut context).cast::<c_void>();

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
    // constructors. `info` points to `context`, which outlives the observer and the timer:
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
            CFAbsoluteTimeGetCurrent() + idle_interval.as_secs_f64(),
            idle_interval.as_secs_f64(),
            0,
            0,
            Some(timer_fired),
            &mut timer_context,
        )
        .expect("creating a run loop timer cannot fail");
        (observer, timer)
    };

    // SAFETY: nothing calls back before the run loop runs, so this is the only access.
    unsafe { (*info.cast::<Context<'_>>()).timer = Some(timer.clone()) };

    // SAFETY: the mode constant is a valid static.
    let mode = unsafe { kCFRunLoopDefaultMode };
    run_loop.add_observer(Some(&observer), mode);
    run_loop.add_timer(Some(&timer), mode);

    CFRunLoop::run();

    observer.invalidate();
    timer.invalidate();
}
