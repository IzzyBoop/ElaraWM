# ElaraWM

A scrollable-tiling window manager for macOS 27, ported from
[niri](https://github.com/niri-wm/niri).

ElaraWM runs niri's own layout and configuration code. The Wayland compositor backend is replaced
by a macOS backend that arranges real application windows through the Accessibility API, so it
works with System Integrity Protection enabled.

**Status:** early development. Windows are tiled into niri's scrolling columns, on niri's dynamic
workspaces, and driven by niri's key bindings and config file. Nothing is drawn yet: no focus
ring, borders or animations. It has had very little use on a real desktop.

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

## Configuration

ElaraWM reads `~/.config/elara/config.kdl`, in
[niri's format](https://niri-wm.github.io/niri/Configuration:-Introduction). The file is created
on first start from niri's default config, adjusted for macOS, and is reloaded whenever it
changes. If an edit has an error, the error is printed and the previous config stays in use.

- `Mod` in binds is the Option key. Set `mod-key` in the `input` section to change it. Command
  is `Super`, Option is `Alt`.
- Holding `Mod` and scrolling with two fingers on the trackpad drags the strip of columns
  sideways, or the workspaces up and down, like niri's three-finger swipe. The mouse wheel
  triggers the `WheelScroll…` binds; by default `Mod`+wheel switches workspaces and
  `Mod+Shift`+wheel moves between columns.
- Window rules match `app-id` against the application's bundle identifier, such as
  `com.apple.Terminal`.
- Output names are display names, such as `"Built-in Retina Display"`.
- Sections for things macOS controls itself (display modes, keyboard layout, pointer settings,
  the cursor, layer rules) are accepted and ignored, with a note in the log.

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
