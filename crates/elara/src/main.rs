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

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("elara: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let (config, config_path) = match load_config(args.config) {
        Ok(loaded) => loaded,
        Err(message) => {
            eprintln!("elara: the config could not be loaded:\n{message}");
            return ExitCode::FAILURE;
        }
    };

    run(config, config_path)
}

#[cfg(target_os = "macos")]
fn run(config: Config, config_path: Option<PathBuf>) -> ExitCode {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use elara::keys::KeyHandler;
    use elara::State;
    use elara_macos::{request_accessibility, run_main_loop, KeyTap, MacBackend};

    // Shows the system dialog pointing at the right settings pane if the permission is missing.
    if !request_accessibility() {
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
            eprintln!("elara: could not start: {err}");
            return ExitCode::FAILURE;
        }
    };

    // On Ctrl-C or a termination request, leave through the normal path so that windows on
    // other workspaces are brought back on screen first.
    let quit = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if let Err(err) = signal_hook::flag::register(signal, Arc::clone(&quit)) {
            tracing::warn!("could not install handler for signal {signal}: {err}");
        }
    }

    elara::spawn_at_startup(&config);
    let mut watcher = config_path.map(config::ConfigWatcher::new);

    let state = Rc::new(RefCell::new(State::new(backend, config)));
    tracing::info!(
        "managing {} windows",
        state.borrow().layout().windows().count()
    );

    // Key presses arrive through the tap, between passes of the loop below. The tap only
    // decides whether a press is a bind; the action waits here until the loop runs it.
    let actions = Rc::new(RefCell::new(VecDeque::new()));
    let mut key_handler = KeyHandler::new();
    let tap = KeyTap::new({
        let state = Rc::clone(&state);
        let actions = Rc::clone(&actions);
        move |event| {
            // The state is only ever borrowed elsewhere while the loop below runs a pass,
            // and the tap does not fire during one. If that ever changes, let the key through
            // rather than panic inside a system callback.
            let Ok(state) = state.try_borrow() else {
                return false;
            };
            let response = key_handler.handle(state.config(), event);
            if response.action.is_some() {
                tracing::debug!(repeat = event.is_repeat, "bound key pressed");
            }
            if let Some(action) = response.action {
                actions.borrow_mut().push_back(action);
            }
            response.swallow
        }
    });
    if tap.is_none() {
        tracing::warn!("could not install the key event tap; key bindings will not work");
    }

    let mut crashed = false;
    // The interval bounds how long a signal waits to be noticed and how often applications
    // that were not ready to be observed are retried.
    run_main_loop(Duration::from_millis(250), || {
        if quit.load(Ordering::Relaxed) {
            return false;
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

            let queued: Vec<_> = actions.borrow_mut().drain(..).collect();
            for action in queued {
                state.do_action(action);
            }
            state.tick();
        }));
        if pass.is_err() {
            crashed = true;
            return false;
        }
        !state.should_quit()
    });

    // Stop intercepting keys before anything else.
    drop(tap);
    let mut state = state.borrow_mut();

    tracing::info!("bringing windows back on screen");
    if catch_unwind(AssertUnwindSafe(|| state.release_windows())).is_err() {
        crashed = true;
    }

    if crashed {
        eprintln!("elara: stopped after an internal error; see the message above.");
        ExitCode::from(101)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(not(target_os = "macos"))]
fn run(_config: Config, _config_path: Option<PathBuf>) -> ExitCode {
    eprintln!("elara: only macOS is supported.");
    ExitCode::FAILURE
}
