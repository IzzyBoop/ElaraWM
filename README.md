# ElaraWM

A scrollable-tiling window manager for macOS 27, ported from
[niri](https://github.com/niri-wm/niri).

ElaraWM runs niri's own layout and configuration code. The Wayland compositor backend is replaced
by a macOS backend that arranges real application windows through the Accessibility API, so it
works with System Integrity Protection enabled.

**Status:** early development. Windows are tiled into niri's scrolling columns as they open and
close. There are no key bindings yet, so the layout cannot be driven from the keyboard.

## Goals

- The same scrolling layout as niri: columns on an infinite horizontal strip, dynamic workspaces
  per monitor.
- niri's `config.kdl` format, read from `~/.config/elara/config.kdl`.
- No SIP changes, no private APIs.

## What macOS does not allow

ElaraWM is a window manager, not a compositor. It cannot change other applications' shadows,
opacity or corner radius, and it cannot draw windows itself. Sections of a niri config that only
make sense on Linux are accepted and ignored.

## Running

```sh
cargo run -p elara
```

ElaraWM needs the Accessibility permission to move other applications' windows. macOS grants it
per application; for a build run from a terminal, that is the terminal (or the editor whose
terminal you use). On first start macOS offers to open the right pane of System Settings.

Stop it with Ctrl-C. Windows that were scrolled out of view are brought back on screen.

To see what ElaraWM can see without managing anything:

```sh
cargo run -p elara-macos --example probe
```

## Building

Requires [rustup](https://rustup.rs). The compiler version is pinned in `rust-toolchain.toml`.

```sh
cargo build
cargo test --workspace
```

## License

GPL-3.0-or-later, the same as niri. See [LICENSE](LICENSE) and [UPSTREAM.md](UPSTREAM.md).
