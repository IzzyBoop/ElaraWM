use std::path::PathBuf;
use std::process::ExitCode;

use elara::config;
use niri_config::Config;

const USAGE: &str = "\
Usage: elara [OPTIONS]

Options:
  -c, --config <PATH>  Read the config from PATH instead of ~/.config/elara/config.kdl
  -h, --help           Print this help
  -V, --version        Print the version

Logging is controlled by the RUST_LOG environment variable, for example RUST_LOG=elara=debug.";

struct Args {
    config: Option<PathBuf>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut config = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("elara {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-c" | "--config" => {
                let path = args.next().ok_or("--config needs a path")?;
                config = Some(PathBuf::from(path));
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(Some(Args { config }))
}

/// Loads the config and says which file to watch for changes.
///
/// A path given on the command line must exist. The default path is created with the default
/// config if it does not.
fn load_config(explicit: Option<PathBuf>) -> Result<(Config, Option<PathBuf>), String> {
    let (config, path) = match explicit {
        Some(path) => {
            tracing::info!("loading config from {}", path.display());
            (config::load(&path)?, Some(path))
        }
        None => match config::default_path() {
            Some(path) => {
                let (config, created) = config::load_or_create(&path)?;
                if created {
                    tracing::info!("created a default config at {}", path.display());
                } else {
                    tracing::info!("loaded config from {}", path.display());
                }
                (config, Some(path))
            }
            None => {
                tracing::warn!("HOME is not set; using the built-in default config");
                (config::default_config(), None)
            }
        },
    };

    warn_about_ignored_sections(&config);
    Ok((config, path))
}

fn warn_about_ignored_sections(config: &Config) {
    for section in config::ignored_sections(config) {
        tracing::warn!("config section has no effect on macOS: {section}");
    }
}

/// Sends everything written to standard error to a log file if ElaraWM was started as an
/// application, where there is no terminal to read it in. Returns the file if so.
///
/// The log of the previous run is kept next to it, with `.1` added to its name.
#[cfg(target_os = "macos")]
fn log_to_file_without_terminal() -> Option<PathBuf> {
    use std::io::IsTerminal;
    use std::os::fd::AsRawFd;

    if std::io::stderr().is_terminal() || elara_macos::bundle_identifier().is_none() {
        return None;
    }
    let dir = PathBuf::from(std::env::var_os("HOME")?).join("Library/Logs/ElaraWM");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("elara.log");
    let _ = std::fs::rename(&path, dir.join("elara.log.1"));
    // For its owner only: the log names the home directory and, when asked for more detail,
    // the applications that have windows open.
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .ok()?
    };
    // Standard error itself, so that the message of a panic lands in the file too.
    // SAFETY: both are open file descriptors, and nothing else is running yet that could be
    // writing to standard error.
    let result = unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) };
    (result != -1).then_some(path)
}

#[cfg(not(target_os = "macos"))]
fn log_to_file_without_terminal() -> Option<PathBuf> {
    None
}

/// Reports why ElaraWM cannot go on: on standard error, and in a dialog if that is a file
/// nobody is looking at.
fn fail(log: Option<&PathBuf>, message: &str) {
    eprintln!("elara: {message}");
    #[cfg(target_os = "macos")]
    if let Some(log) = log {
        let text = format!("{message}\n\nThe log is at {}.", log.display());
        elara_macos::alert("ElaraWM could not start", &text);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = log;
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("elara: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let log = log_to_file_without_terminal();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(log.is_none())
        .init();

    let (config, config_path) = match load_config(args.config) {
        Ok(loaded) => loaded,
        Err(message) => {
            fail(
                log.as_ref(),
                &format!("The config could not be loaded:\n{message}"),
            );
            return ExitCode::FAILURE;
        }
    };

    run(config, config_path, log)
}

#[cfg(target_os = "macos")]
fn run(config: Config, config_path: Option<PathBuf>, log: Option<PathBuf>) -> ExitCode {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use elara::keys::KeyHandler;
    use elara::pointer::{PointerHandler, PointerInput};
    use elara::scroll::{ScrollHandler, ScrollInput};
    use elara::State;
    use elara_macos::{
        accessibility_trusted, request_accessibility, run_main_loop, take_overview_input,
        InputEvent, KeyTap, MacBackend, MenuBar, MenuCommand, MouseButton, MouseEventKind,
        OverviewInput, Wake,
    };
    use niri_config::Action;

    /// How long to wait between frames of an animation. Each frame moves windows in other
    /// applications, which takes a few milliseconds, so this is the pause between frames
    /// rather than their period.
    const FRAME_INTERVAL: Duration = Duration::from_millis(8);

    /// Something the input tap took, waiting for the loop to carry it out.
    enum Input {
        Scroll(ScrollInput),
        Pointer(PointerInput),
        /// The left button went down or came up on something that is not ElaraWM's.
        SystemPress(bool),
        /// Take the list of key bindings away: something was pressed or scrolled.
        HideHotkeyOverlay,
    }

    /// The mark shown in the menu bar.
    const MARK: &[u8] = include_bytes!("../../../resources/icons/elara.svg");

    // On Ctrl-C or a termination request, leave through the normal path so that windows on
    // other workspaces are brought back on screen first.
    let quit = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if let Err(err) = signal_hook::flag::register(signal, Arc::clone(&quit)) {
            tracing::warn!("could not install handler for signal {signal}: {err}");
        }
    }

    let mut menu = MenuBar::new(MARK);
    menu.set_log_available(log.is_some());
    menu.set_start_at_login(elara_macos::start_at_login());

    // Carries out what was chosen from the menu. Returns whether that was to quit.
    let run_menu_commands = {
        let config_path = config_path.clone();
        let log = log.clone();
        move |menu: &mut MenuBar| {
            let mut quit = false;
            for command in menu.take_commands() {
                match command {
                    MenuCommand::OpenConfig => match &config_path {
                        // In a text editor: nothing is registered to open `.kdl` files.
                        Some(path) => elara::spawn(vec![
                            "/usr/bin/open".to_owned(),
                            "-t".to_owned(),
                            path.display().to_string(),
                        ]),
                        None => tracing::warn!("there is no config file to open"),
                    },
                    MenuCommand::ShowLog => {
                        if let Some(log) = &log {
                            elara::spawn(vec![
                                "/usr/bin/open".to_owned(),
                                log.display().to_string(),
                            ]);
                        }
                    }
                    MenuCommand::ToggleStartAtLogin => {
                        let was_on = elara_macos::start_at_login().unwrap_or(false);
                        match elara_macos::set_start_at_login(!was_on) {
                            Ok(on) => {
                                if !was_on && !on {
                                    tracing::warn!(
                                        "ElaraWM is switched off in System Settings > General > \
                                         Login Items; switch it on there to start it at login"
                                    );
                                }
                                menu.set_start_at_login(Some(on));
                            }
                            Err(message) => {
                                tracing::error!("could not change starting at login: {message}");
                            }
                        }
                    }
                    MenuCommand::Quit => quit = true,
                }
            }
            quit
        }
    };

    // Shows the system dialog pointing at the right settings pane if the permission is missing.
    if !request_accessibility() {
        if log.is_none() {
            eprintln!(
                "elara: the Accessibility permission has not been granted.\n\
                 \n\
                 ElaraWM moves and resizes other applications' windows, which macOS only allows\n\
                 with this permission. Grant it to the application you launched elara from (your\n\
                 terminal or editor) in System Settings > Privacy & Security > Accessibility,\n\
                 then start elara again."
            );
            return ExitCode::FAILURE;
        }

        // Started as an application: the permission is ElaraWM's own, and the user is on
        // their way to grant it. Wait in the menu bar until they have.
        tracing::info!("waiting for the Accessibility permission");
        menu.set_status("Waiting for the Accessibility permission");
        let mut granted = false;
        run_main_loop(Duration::from_millis(500), || {
            if run_menu_commands(&mut menu) || quit.load(Ordering::Relaxed) {
                return Wake::Stop;
            }
            granted = accessibility_trusted();
            if granted {
                Wake::Stop
            } else {
                Wake::WhenNeeded
            }
        });
        if !granted {
            return ExitCode::SUCCESS;
        }
        tracing::info!("the Accessibility permission was granted");
    }

    match elara_macos::displays_have_separate_spaces() {
        Some(true) => tracing::debug!("displays have separate Spaces"),
        Some(false) => tracing::info!(
            "\"Displays have separate Spaces\" is off: the displays form one desktop, and a \
             window at the edge of one display can show on the next"
        ),
        None => tracing::debug!("could not read the \"Displays have separate Spaces\" setting"),
    }

    let backend = match MacBackend::new() {
        Ok(backend) => backend,
        Err(err) => {
            fail(log.as_ref(), &format!("Could not start: {err}"));
            return ExitCode::FAILURE;
        }
    };

    elara::spawn_at_startup(&config);
    let mut watcher = config_path.map(config::ConfigWatcher::new);

    let state = Rc::new(RefCell::new(State::new(backend, config)));
    // Windows were put in place at once above. From here on, the strip and the workspaces
    // glide.
    state.borrow_mut().enable_animations();
    tracing::info!(
        "managing {} windows",
        state.borrow().layout().windows().count()
    );

    // Keys, scrolling and the mouse arrive through the tap, between passes of the loop below. The
    // tap only decides whether an event is for ElaraWM; what it asks for waits here until the loop
    // carries it out. Finger and pointer movement is queued like everything else, so that
    // however many such events arrive while windows are being moved, they are moved once.
    let inputs = Rc::new(RefCell::new(VecDeque::new()));
    let mut key_handler = KeyHandler::new();
    let mut scroll_handler = ScrollHandler::new();
    let mut pointer_handler = PointerHandler::new();
    let tap = KeyTap::new({
        let state = Rc::clone(&state);
        let inputs = Rc::clone(&inputs);
        move |event| {
            // The state is only ever borrowed elsewhere while the loop below runs a pass,
            // and the tap does not fire during one. If that ever changes, let the event
            // through rather than panic inside a system callback.
            let Ok(state) = state.try_borrow() else {
                return false;
            };
            // As in niri, pressing or scrolling anything takes the list of key bindings away,
            // after whatever else it does. That includes the key that shows the list, which
            // makes it a toggle. The list counts as showing if a key that shows it is still
            // waiting for the loop.
            let hides_overlay = elara::ui::hides_hotkey_overlay(event)
                && (state.hotkey_overlay_open()
                    || inputs.borrow().iter().any(|input| {
                        matches!(
                            input,
                            Input::Scroll(ScrollInput::Action(Action::ShowHotkeyOverlay))
                        )
                    }));
            // On another macOS Space, the keys and the mouse are not ElaraWM's.
            let swallow = if state.is_dormant() {
                false
            } else {
                match event {
                    InputEvent::Key(event) => {
                        let overview_open = state.layout().is_overview_open();
                        let response =
                            key_handler.handle_with_overview(state.config(), event, overview_open);
                        if let Some(action) = response.action {
                            tracing::debug!(repeat = event.is_repeat, "bound key pressed");
                            let input = Input::Scroll(ScrollInput::Action(action));
                            inputs.borrow_mut().push_back(input);
                        } else if event.pressed
                            && (event.modifiers.control
                                || event.modifiers.option
                                || event.modifiers.command)
                        {
                            // Only what could have been a shortcut, never what is being
                            // typed: a key with no modifier, or with Shift alone, is text.
                            tracing::trace!(
                                keycode = event.keycode,
                                modifiers = ?event.modifiers,
                                has_character = event.character.is_some(),
                                "key with modifiers is not bound"
                            );
                        }
                        response.swallow
                    }
                    InputEvent::Scroll(event) => {
                        let response = scroll_handler.handle(state.config(), event, Instant::now());
                        if let Some(input) = response.input {
                            inputs.borrow_mut().push_back(Input::Scroll(input));
                        }
                        response.swallow
                    }
                    InputEvent::Mouse(event) => {
                        let response = pointer_handler.handle(
                            state.config(),
                            event,
                            |button, position, mod_down| {
                                state.takes_press(button, position, mod_down)
                            },
                        );
                        let mut inputs = inputs.borrow_mut();
                        inputs.extend(response.inputs.into_iter().map(Input::Pointer));
                        // A left press that is not for ElaraWM may be the start of the system
                        // dragging a window.
                        if !response.swallow {
                            match event.kind {
                                MouseEventKind::Pressed(MouseButton::Left) => {
                                    inputs.push_back(Input::SystemPress(true));
                                }
                                MouseEventKind::Released(MouseButton::Left) => {
                                    inputs.push_back(Input::SystemPress(false));
                                }
                                _ => {}
                            }
                        }
                        if let Some(action) = response.action {
                            tracing::debug!("bound mouse button pressed");
                            inputs.push_back(Input::Scroll(ScrollInput::Action(action)));
                        }
                        response.swallow
                    }
                }
            };
            if hides_overlay {
                inputs.borrow_mut().push_back(Input::HideHotkeyOverlay);
            }
            swallow
        }
    });
    if tap.is_none() {
        tracing::warn!(
            "could not install the input event tap; key bindings, scrolling and dragging \
             windows will not work"
        );
        // Nothing could take the list of key bindings away again.
        state.borrow_mut().hide_hotkey_overlay();
    }

    // The overview is a window of ElaraWM's own. What is scrolled on it comes from that
    // window, not through the tap, and has its own bookkeeping.
    let mut overview_scroll = ScrollHandler::new();

    let mut crashed = false;
    // The interval bounds how long a signal waits to be noticed and how often applications
    // that were not ready to be observed are retried.
    run_main_loop(Duration::from_millis(250), || {
        if run_menu_commands(&mut menu) || quit.load(Ordering::Relaxed) {
            return Wake::Stop;
        }
        let mut state = state.borrow_mut();
        // A panic must not unwind into the run loop, and must not leave windows parked off
        // screen with nothing running that could bring them back.
        let pass = catch_unwind(AssertUnwindSafe(|| {
            // Pick up edits to the config file. A config with errors is reported and the one
            // in use stays, as in niri.
            let changed = watcher
                .as_mut()
                .and_then(|watcher| watcher.changed().then(|| watcher.path().to_owned()));
            if let Some(path) = changed {
                match config::load(&path) {
                    Ok(config) => {
                        tracing::info!("config changed, reloading");
                        warn_about_ignored_sections(&config);
                        state.set_config(config);
                    }
                    Err(message) => {
                        tracing::error!(
                            "the config has errors; keeping the previous one:\n{message}"
                        );
                    }
                }
            }

            let queued: Vec<_> = inputs.borrow_mut().drain(..).collect();
            for input in queued {
                match input {
                    Input::Scroll(ScrollInput::Action(action)) => {
                        state.do_action(action);
                    }
                    Input::Scroll(ScrollInput::GestureUpdate { delta_x, delta_y }) => {
                        state.gesture_update(delta_x, delta_y);
                    }
                    Input::Scroll(ScrollInput::GestureEnd) => state.gesture_end(),
                    Input::Pointer(input) => state.pointer_input(input),
                    Input::SystemPress(down) => state.system_press(down),
                    Input::HideHotkeyOverlay => state.hide_hotkey_overlay(),
                }
            }
            for input in take_overview_input() {
                match input {
                    OverviewInput::Mouse(event) => {
                        state.pointer_input(PointerInput::from_overview(&event));
                    }
                    OverviewInput::Scroll(event, position) => {
                        let input = overview_scroll.handle_in_overview(&event, Instant::now());
                        if let Some(input) = input {
                            state.overview_scroll(input, position);
                        }
                    }
                }
            }
            state.tick();
        }));
        if pass.is_err() {
            crashed = true;
            return Wake::Stop;
        }
        if let Some(label) = state.workspace_label() {
            menu.set_workspace(&label);
        }
        if state.should_quit() {
            Wake::Stop
        } else if state.is_animating() {
            Wake::After(FRAME_INTERVAL)
        } else {
            Wake::WhenNeeded
        }
    });

    // Stop intercepting keys before anything else.
    drop(tap);
    drop(menu);
    let mut state = state.borrow_mut();

    tracing::info!("bringing windows back on screen");
    if catch_unwind(AssertUnwindSafe(|| state.release_windows())).is_err() {
        crashed = true;
    }

    if crashed {
        eprintln!("elara: stopped after an internal error; see the message above.");
        if let Some(log) = &log {
            let text = format!("The log has the details: {}", log.display());
            elara_macos::alert("ElaraWM stopped after an internal error", &text);
        }
        ExitCode::from(101)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(not(target_os = "macos"))]
fn run(_config: Config, _config_path: Option<PathBuf>, _log: Option<PathBuf>) -> ExitCode {
    eprintln!("elara: only macOS is supported.");
    ExitCode::FAILURE
}
