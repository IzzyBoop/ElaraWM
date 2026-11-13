use std::path::PathBuf;
use std::process::ExitCode;

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

/// Loads the config, falling back to niri's built-in default when there is no file.
fn load_config(explicit: Option<PathBuf>) -> Result<Config, String> {
    let path = match explicit {
        Some(path) => path,
        None => {
            let Some(home) = std::env::var_os("HOME") else {
                return Ok(Config::load_default());
            };
            let path = PathBuf::from(home).join(".config/elara/config.kdl");
            if !path.exists() {
                tracing::info!(
                    "no config at {}, using the built-in default",
                    path.display()
                );
                return Ok(Config::load_default());
            }
            path
        }
    };

    tracing::info!("loading config from {}", path.display());
    Config::load(&path).config.map_err(|err| format!("{err:?}"))
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

    let config = match load_config(args.config) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("elara: the config could not be loaded:\n{message}");
            return ExitCode::FAILURE;
        }
    };

    run(config)
}

#[cfg(target_os = "macos")]
fn run(config: Config) -> ExitCode {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use elara::State;
    use elara_macos::{request_accessibility, run_main_loop, MacBackend};

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

    let mut state = State::new(backend, config);
    tracing::info!("managing {} windows", state.layout().windows().count());

    let mut crashed = false;
    // The interval bounds how long a signal waits to be noticed and how often applications
    // that were not ready to be observed are retried.
    run_main_loop(Duration::from_millis(250), || {
        if quit.load(Ordering::Relaxed) {
            return false;
        }
        // A panic must not unwind into the run loop, and must not leave windows parked off
        // screen with nothing running that could bring them back.
        if catch_unwind(AssertUnwindSafe(|| state.tick())).is_err() {
            crashed = true;
            return false;
        }
        true
    });

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
fn run(_config: Config) -> ExitCode {
    eprintln!("elara: only macOS is supported.");
    ExitCode::FAILURE
}
