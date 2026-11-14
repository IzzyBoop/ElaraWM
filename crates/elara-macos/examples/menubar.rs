//! Shows ElaraWM's menu bar item on its own, to check how it looks and that its menu works.
//!
//! ```sh
//! cargo run -p elara-macos --example menubar -- 20
//! ```
//!
//! The item stays for the given number of seconds (5 if none is given) and whatever is
//! chosen from its menu is printed. Nothing is done about the choice, except that Quit ends
//! the example. It needs no permission and touches no windows.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use elara_macos::{run_main_loop, MenuBar, MenuCommand, Wake};

    let seconds: f64 = std::env::args()
        .nth(1)
        .map(|arg| arg.parse().expect("the argument is a number of seconds"))
        .unwrap_or(5.);

    let mark = include_bytes!("../../../resources/icons/elara.svg");
    let mut bar = MenuBar::new(mark);

    // The daemon runs the loop twice when it has to wait for the permission first, with
    // this in the menu meanwhile.
    bar.set_status("Waiting for the Accessibility permission");
    let waiting = Instant::now();
    run_main_loop(Duration::from_millis(100), || {
        if waiting.elapsed() > Duration::from_secs(1) {
            Wake::Stop
        } else {
            Wake::WhenNeeded
        }
    });
    println!("the first run of the loop returned");

    bar.set_workspace("2");
    bar.set_start_at_login(Some(false));

    let started = Instant::now();
    let mut reported = false;
    let mut start_at_login = false;
    run_main_loop(Duration::from_millis(100), || {
        if !reported && started.elapsed() > Duration::from_millis(500) {
            reported = true;
            println!("shown in the menu bar: {}", bar.is_shown());
        }
        for command in bar.take_commands() {
            println!("chosen: {command:?}");
            match command {
                MenuCommand::ToggleStartAtLogin => {
                    start_at_login = !start_at_login;
                    bar.set_start_at_login(Some(start_at_login));
                }
                MenuCommand::Quit => return Wake::Stop,
                _ => {}
            }
        }
        if started.elapsed().as_secs_f64() >= seconds {
            Wake::Stop
        } else {
            Wake::After(Duration::from_millis(100))
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("only macOS is supported");
}
