//! Prints what ElaraWM can see: the Accessibility permission, displays and windows.
//!
//! ```sh
//! cargo run -p elara-macos --example probe            # report only
//! cargo run -p elara-macos --example probe -- --ask   # also show the permission dialog
//! ```
//!
//! Window titles are not printed unless `--titles` is passed.

#[cfg(target_os = "macos")]
fn main() {
    use elara_macos::{
        accessibility_trusted, request_accessibility, Error, MacBackend, WindowBackend,
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let ask = args.iter().any(|arg| arg == "--ask");
    let titles = args.iter().any(|arg| arg == "--titles");

    let trusted = if ask {
        request_accessibility()
    } else {
        accessibility_trusted()
    };
    println!(
        "Accessibility permission: {}",
        if trusted { "granted" } else { "not granted" }
    );
    // Only read: asking would put a dialog on screen.
    println!(
        "Screen Recording permission (for window-thumbnails): {}",
        if elara_macos::screen_recording_permitted() {
            "granted"
        } else {
            "not granted"
        }
    );

    println!(
        "Displays have separate Spaces: {}",
        match elara_macos::displays_have_separate_spaces() {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        }
    );

    println!();
    println!("Displays:");
    for d in elara_macos::displays() {
        println!(
            "  {} (id {}){}: {}x{} at ({}, {}), usable {}x{} at ({}, {}), scale {}, {} Hz",
            d.name,
            d.id.0,
            if d.is_primary { ", primary" } else { "" },
            d.frame.w,
            d.frame.h,
            d.frame.x,
            d.frame.y,
            d.visible_frame.w,
            d.visible_frame.h,
            d.visible_frame.x,
            d.visible_frame.y,
            d.scale,
            d.refresh_mhz
                .map_or("?".to_owned(), |mhz| (mhz / 1000).to_string()),
        );
    }

    let backend = match MacBackend::new() {
        Ok(backend) => backend,
        Err(Error::NotTrusted) => {
            println!();
            println!("Without the permission, windows cannot be listed or moved.");
            println!("Grant it to the application running this command (your terminal or editor)");
            println!("in System Settings > Privacy & Security > Accessibility, then run it again.");
            println!("Pass --ask to have macOS show its dialog with a shortcut to that pane.");
            return;
        }
        Err(err) => {
            println!("could not start: {err}");
            return;
        }
    };

    println!();
    println!(
        "Can tell macOS Spaces apart: {}",
        if backend.can_tell_spaces() {
            "yes"
        } else {
            "no (every window is taken to be on this Space)"
        }
    );

    let focused = backend.focused_window();
    let mut windows = backend.windows();
    windows.sort_by_key(|w| w.id);
    println!();
    println!("Windows ({}):", windows.len());
    for w in windows {
        println!(
            "  #{:<3} {:<34} {:?} {}x{} at ({}, {}){}{}{}{}{}{}{}",
            w.id.0,
            w.app_id.as_deref().unwrap_or("(no bundle id)"),
            w.kind,
            w.frame.w,
            w.frame.h,
            w.frame.x,
            w.frame.y,
            if w.resizable { "" } else { " fixed-size" },
            if w.movable { "" } else { " immovable" },
            if w.minimized { " minimized" } else { "" },
            if w.fullscreen { " fullscreen" } else { "" },
            if w.on_other_space {
                " on-another-Space"
            } else {
                ""
            },
            if focused == Some(w.id) {
                " focused"
            } else {
                ""
            },
            match (&w.title, titles) {
                (Some(title), true) => format!(" \"{title}\""),
                _ => String::new(),
            },
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("This example only does something on macOS.");
}
