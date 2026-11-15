//! Checks the drawing of the hotkey overlay: the list of important key bindings.
//!
//! ```sh
//! cargo run -p elara --example hotkey_overlay -- --raster out.rgba   # draw into a file
//! cargo run -p elara --example hotkey_overlay -- --show 3            # draw on screen for 3 s
//! ```
//!
//! Both show the bindings of the default config; add `--config <path>` for those of another.
//! `--raster` puts nothing on screen. It writes a line with the width and height in pixels,
//! then the pixels as bytes of red, green, blue and alpha. `--show` puts the list in the
//! middle of every display. Neither moves a window or needs the Accessibility permission.

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use elara::binds::mod_key;
    use elara::config;
    use elara::ui::hotkey_overlay::HotkeyOverlay;
    use elara_macos::{displays, render_hotkey_overlay, run_main_loop, HotkeyOverlayPanels, Wake};

    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut config = config::default_config();
    if let Some(at) = args.iter().position(|arg| arg == "--config") {
        let path = args.get(at + 1).expect("--config takes a path");
        config = config::load(std::path::Path::new(path)).expect("the config must load");
        args.drain(at..at + 2);
    }

    let mut overlay = HotkeyOverlay::new(mod_key(&config));
    overlay.show();
    let content = overlay.content(&config).expect("the overlay is open");

    let displays = displays();
    match args.as_slice() {
        [flag, path] if flag == "--raster" => {
            let scale = displays.first().map_or(2., |display| display.scale);
            let (width, height, pixels) = render_hotkey_overlay(&content, scale);
            let mut file = std::fs::File::create(path).expect("could not create the file");
            writeln!(file, "{width} {height}").unwrap();
            file.write_all(&pixels).unwrap();
            println!("wrote {width}x{height} pixels to {path}");
        }
        [flag, seconds] if flag == "--show" => {
            let seconds: f64 = seconds.parse().expect("--show takes a number of seconds");
            let mut panels = HotkeyOverlayPanels::new();
            panels.set(&displays, Some(&content));

            let started = Instant::now();
            run_main_loop(Duration::from_millis(100), || {
                if started.elapsed().as_secs_f64() >= seconds {
                    Wake::Stop
                } else {
                    Wake::After(Duration::from_millis(100))
                }
            });
            panels.set(&displays, None);
        }
        _ => {
            eprintln!("usage: hotkey_overlay --raster <file> | --show <seconds>  [--config <path>]")
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
