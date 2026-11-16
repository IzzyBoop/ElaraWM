//! Checks the pictures of windows that the overview shows when `window-thumbnails` is on.
//!
//! ```sh
//! cargo run -p elara-macos --example thumbnails -- 4          # show for 4 seconds
//! cargo run -p elara-macos --example thumbnails -- 4 --ask    # let macOS ask for permission
//! ```
//!
//! Covers the primary display with an overview that has the real windows side by side in
//! rows, each at a quarter of its size, and prints how many of them a picture was taken of.
//! No window is moved. It needs the Accessibility permission to list the windows and the
//! Screen Recording permission to take the pictures; without `--ask` it stops if the second
//! is missing, so that no dialog comes up unasked.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use elara_macos::{
        run_main_loop, screen_recording_permitted, MacBackend, Overview, OverviewDisplay,
        OverviewItem, OverviewWindow, Rect, Wake, WindowBackend,
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: f64 = args
        .first()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(4.);
    let ask = args.iter().any(|arg| arg == "--ask");

    if !screen_recording_permitted() && !ask {
        println!("Screen Recording permission: not granted. Pass --ask to have macOS ask.");
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

    let zoom = 0.25;
    let gap = 16.;
    let (mut x, mut y, mut row) = (display.frame.x + gap, display.frame.y + gap, 0f64);
    let mut items = Vec::new();
    for window in backend.windows() {
        if window.minimized || window.on_other_space {
            continue;
        }
        let (w, h) = (window.frame.w * zoom, window.frame.h * zoom);
        if x + w > display.frame.x + display.frame.w {
            x = display.frame.x + gap;
            y += row + gap;
            row = 0.;
        }
        items.push(OverviewItem::Window(OverviewWindow {
            id: window.id,
            rect: Rect::new(x, y, w, h),
            corner_radius: [16. * zoom; 4],
            title: window.title,
        }));
        x += w + gap;
        row = row.max(h);
    }
    let windows = items.len();
    let overview = Overview {
        backdrop: [0.15, 0.15, 0.15, 1.],
        backdrop_image: None,
        window_opacity: 1.,
        thumbnails: Some(zoom),
        displays: vec![OverviewDisplay {
            display: display.id,
            workspaces: Vec::new(),
            items,
        }],
    };

    let started = Instant::now();
    run_main_loop(Duration::from_millis(100), || {
        backend.drain_events();
        backend.set_overview(Some(&overview));
        if started.elapsed().as_secs_f64() >= seconds {
            Wake::Stop
        } else {
            Wake::WhenNeeded
        }
    });
    println!(
        "pictures of {} of {windows} windows",
        backend.thumbnail_count()
    );
    backend.set_overview(None);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
