//! Checks the drawing of outlines, the shapes behind focus rings, borders and the insert hint.
//!
//! ```sh
//! cargo run -p elara-macos --example overlay -- --raster out.rgba   # draw into a file
//! cargo run -p elara-macos --example overlay -- --show 2            # draw on screen for 2 s
//! ```
//!
//! `--raster` puts nothing on screen. It writes a line with the width and height in pixels,
//! then the pixels as bytes of red, green, blue and alpha. `--show` draws the same shapes on
//! the primary display and reports whether the system lists the overlay window as on screen.
//! Neither needs the Accessibility permission.

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use elara_macos::{
        displays, rasterize, run_main_loop, ColorSpace, Gradient, HueInterpolation, Outline,
        Overlay, Rect, Size, Wake,
    };

    // A focus ring around a window with round corners, a border with square ones, and an
    // insert hint, at the sizes niri's default config gives them. The ring has the gradient
    // that niri's default config suggests, from the bottom left to the top right.
    let sample = |x: f64, y: f64| {
        vec![
            Outline {
                rect: Rect::new(x + 20., y + 20., 300., 200.),
                line_width: 4.,
                corner_radius: [16.; 4],
                color: [0.502, 0.784, 1., 1.],
                gradient: Some(Gradient {
                    to: [0.78, 1., 0.498, 1.],
                    area: Rect::new(x + 20., y + 20., 300., 200.),
                    angle: (45f64 - 90.).to_radians(),
                    space: ColorSpace::Srgb,
                    hue: HueInterpolation::Shorter,
                }),
            },
            Outline {
                rect: Rect::new(x + 360., y + 20., 200., 200.),
                line_width: 4.,
                corner_radius: [0.; 4],
                color: [0.314, 0.314, 0.314, 1.],
                gradient: None,
            },
            Outline {
                rect: Rect::new(x + 600., y + 20., 120., 200.),
                line_width: 0.,
                corner_radius: [12., 12., 0., 0.],
                color: [1., 0.784, 0.498, 0.5],
                gradient: None,
            },
        ]
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, path] if flag == "--raster" => {
            let (width, height, pixels) = rasterize(&sample(0., 0.), Size { w: 740., h: 240. }, 2.);
            let mut file = std::fs::File::create(path).expect("could not create the file");
            writeln!(file, "{width} {height}").unwrap();
            file.write_all(&pixels).unwrap();
            println!("wrote {width}x{height} pixels to {path}");
        }
        [flag, seconds] if flag == "--show" => {
            let seconds: f64 = seconds.parse().expect("--show takes a number of seconds");
            let displays = displays();
            let primary = displays.first().expect("no display").clone();
            let mut overlay = Overlay::new();
            overlay.set(
                &displays,
                &sample(
                    primary.visible_frame.x + 100.,
                    primary.visible_frame.y + 100.,
                ),
            );

            let started = Instant::now();
            let mut reported = false;
            run_main_loop(Duration::from_millis(100), || {
                if !reported && started.elapsed() > Duration::from_millis(500) {
                    reported = true;
                    match overlay.on_screen_frames().as_slice() {
                        [] => println!("the system lists no overlay window on screen"),
                        frames => println!("on screen: {frames:?}"),
                    }
                }
                if started.elapsed().as_secs_f64() >= seconds {
                    Wake::Stop
                } else {
                    Wake::After(Duration::from_millis(100))
                }
            });
        }
        _ => eprintln!("usage: overlay --raster <path> | --show <seconds>"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
