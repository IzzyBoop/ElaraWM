//! Checks the drawing of the overview: workspaces, window cards and what goes around them.
//!
//! ```sh
//! cargo run -p elara-macos --example overview -- --raster out.rgba   # draw into a file
//! cargo run -p elara-macos --example overview -- --show 3            # draw on screen for 3 s
//! ```
//!
//! `--raster` puts nothing on screen. It writes a line with the width and height in pixels,
//! then the pixels as bytes of red, green, blue and alpha. `--show` covers every display with
//! the same picture and prints what is clicked and scrolled on it. Add `--pid <pid>` to
//! either to draw the icon of that process on the cards. Neither needs the Accessibility
//! permission.

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use elara_macos::{
        displays, render_overview, run_main_loop, take_overview_input, ColorSpace, DisplayInfo,
        Gradient, HueInterpolation, Outline, Overview, OverviewDisplay, OverviewItem,
        OverviewPanels, OverviewShadow, OverviewWindow, OverviewWorkspace, Rect, Wake, WindowId,
    };

    // Three workspaces at half size, as niri's default zoom shows them: the one in the
    // middle with three columns, of which the last reaches past its edge.
    fn sample(display: &DisplayInfo) -> Overview {
        let frame = display.frame;
        let zoom = 0.5;
        let (w, h) = (frame.w * zoom, frame.h * zoom);
        let gap = frame.h * 0.1 * zoom;
        let x = frame.x + (frame.w - w) / 2.;
        let middle = frame.y + (frame.h - h) / 2.;

        let workspaces = [-1., 0., 1.]
            .map(|idx| OverviewWorkspace {
                rect: Rect::new(x, middle + idx * (h + gap), w, h),
                color: [0.25, 0.25, 0.25, 1.],
                shadow: Some(OverviewShadow {
                    offset: (0., 10. * zoom),
                    softness: 40. * zoom,
                    color: [0., 0., 0., 0.3],
                }),
            })
            .to_vec();

        let column = w / 2. - 12.;
        let window = |id: u64, col: f64, y: f64, height: f64, title: &str| {
            OverviewItem::Window(OverviewWindow {
                id: WindowId(id),
                rect: Rect::new(x + 8. + col * (column + 8.), y, column, height),
                corner_radius: [16. * zoom; 4],
                title: Some(title.to_owned()),
            })
        };
        let top = middle + 8. + 20.;
        let full = h - 16. - 20.;
        let ring = Outline {
            rect: Rect::new(x + 8. - 2., top - 2., column + 4., full + 4.),
            line_width: 2.,
            corner_radius: [16. * zoom + 2.; 4],
            // The gradient niri's default config suggests, from the bottom left to the top
            // right.
            color: [0.502, 0.784, 1., 1.],
            gradient: Some(Gradient {
                to: [0.78, 1., 0.498, 1.],
                area: Rect::new(x + 8. - 2., top - 2., column + 4., full + 4.),
                angle: (45f64 - 90.).to_radians(),
                space: ColorSpace::Srgb,
                hue: HueInterpolation::Shorter,
            }),
        };
        let items = vec![
            OverviewItem::Shape(ring),
            window(1, 0., top, full, "elara: zsh"),
            window(2, 1., top, full / 2. - 4., "Configuration: Overview"),
            window(3, 1., top + full / 2. + 4., full / 2. - 4., "Notes"),
            window(4, 2., top, full, "A window that is mostly out of view"),
            window(
                5,
                0.,
                middle - gap - h + 28.,
                full,
                "On the workspace above",
            ),
        ];

        Overview {
            backdrop: [0.15, 0.15, 0.15, 1.],
            thumbnails: None,
            displays: vec![OverviewDisplay {
                display: display.id,
                workspaces,
                items,
            }],
        }
    }

    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut pid = None;
    if let Some(at) = args.iter().position(|arg| arg == "--pid") {
        pid = args.get(at + 1).and_then(|pid| pid.parse::<i32>().ok());
        args.drain(at..(at + 2).min(args.len()));
    }
    let pid_of = move |_: WindowId| pid;

    let displays = displays();
    let primary = displays.first().expect("no display").clone();
    match args.as_slice() {
        [flag, path] if flag == "--raster" => {
            let (width, height, pixels) = render_overview(&sample(&primary), &primary, &pid_of);
            let mut file = std::fs::File::create(path).expect("could not create the file");
            writeln!(file, "{width} {height}").unwrap();
            file.write_all(&pixels).unwrap();
            println!("wrote {width}x{height} pixels to {path}");
        }
        [flag, seconds] if flag == "--show" => {
            let seconds: f64 = seconds.parse().expect("--show takes a number of seconds");
            let mut panels = OverviewPanels::new();
            panels.set(&displays, Some(&sample(&primary)), &pid_of, &|_| None);

            let started = Instant::now();
            run_main_loop(Duration::from_millis(100), || {
                for input in take_overview_input() {
                    println!("{input:?}");
                }
                if started.elapsed().as_secs_f64() >= seconds {
                    Wake::Stop
                } else {
                    Wake::After(Duration::from_millis(100))
                }
            });
            panels.set(&displays, None, &pid_of, &|_| None);
        }
        _ => eprintln!("usage: overview --raster <file> | --show <seconds>  [--pid <pid>]"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
