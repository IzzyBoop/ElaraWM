//! Checks the pictures of windows that stand in for windows above the menu bar while a
//! workspace slides.
//!
//! ```sh
//! cargo run -p elara-macos --example ghosts -- 3    # slide for 3 seconds
//! ```
//!
//! Takes the first window on the primary display and slides a picture of it down from
//! under the menu bar, to end exactly on top of the window. No window is moved. It needs
//! the Accessibility permission to list the windows and the Screen Recording permission to
//! take the picture, and stops if the second is missing: it never has macOS ask.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use elara_macos::{
        run_main_loop, screen_recording_permitted, Ghost, MacBackend, Rect, Wake, WindowBackend,
    };

    let seconds: f64 = std::env::args()
        .nth(1)
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(3.);

    if !screen_recording_permitted() {
        println!("Screen Recording permission: not granted.");
        return;
    }
    let mut backend = match MacBackend::new() {
        Ok(backend) => backend,
        Err(err) => {
            println!("cannot list windows: {err}");
            return;
        }
    };
    let Some(display) = backend.displays().into_iter().next() else {
        println!("no display");
        return;
    };
    let window = backend.windows().into_iter().find(|window| {
        !window.minimized && !window.on_other_space && window.frame.intersects(&display.frame)
    });
    let Some(window) = window else {
        println!("no window on the primary display");
        return;
    };

    let top = display.visible_frame.y;
    let clip = Rect::new(
        display.frame.x,
        top,
        display.frame.w,
        display.frame.y + display.frame.h - top,
    );
    // From wholly above the menu bar to where the window is.
    let from = top - window.frame.h;
    let to = window.frame.y;

    let started = Instant::now();
    let mut seen = false;
    run_main_loop(Duration::from_millis(8), || {
        backend.drain_events();
        let progress = (started.elapsed().as_secs_f64() / seconds).min(1.);
        let mut rect = window.frame;
        rect.y = from + (to - from) * progress;
        backend.set_ghosts(&[Ghost {
            window: window.id,
            rect,
            clip,
        }]);
        seen |= backend.ghost_picture_count() > 0;
        if progress >= 1. {
            Wake::Stop
        } else {
            Wake::WhenNeeded
        }
    });
    println!(
        "picture of {:?}: {}",
        window.title.as_deref().unwrap_or("?"),
        if seen { "shown" } else { "never came" }
    );
    backend.set_ghosts(&[]);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
