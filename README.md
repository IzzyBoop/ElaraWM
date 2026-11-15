<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="resources/icons/elara-white.svg">
    <img src="resources/icons/elara.svg" alt="ElaraWM" width="220">
  </picture>
</p>

# ElaraWM

A scrollable-tiling window manager for macOS 27, ported from
[niri](https://github.com/niri-wm/niri).

ElaraWM runs niri's own layout and configuration code. The Wayland compositor backend is replaced
by a macOS backend that arranges real application windows through the Accessibility API, so it
works with System Integrity Protection enabled.

**Status:** early development. Windows are tiled into niri's scrolling columns, on niri's dynamic
workspaces, and driven by niri's key bindings and config file. The strip of columns and the
workspaces glide into place. The focus ring, window borders, the tab indicator of tabbed
columns and the hint for where a dragged window will land are drawn; shadows are not. It has had little use on a real desktop.

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

While it runs, ElaraWM has an item in the menu bar: its mark and the name or number of the
workspace that is showing. Its menu opens the config file, shows the log, switches starting at
login on or off, and quits.

### As an application

```sh
tools/bundle-app.sh                 # writes target/release/ElaraWM.app
tools/bundle-app.sh /Applications   # or puts it somewhere else
```

Started this way, ElaraWM has no Dock icon and no window, only the menu bar item. The
Accessibility permission is then its own: on first start macOS offers to open System Settings,
and ElaraWM waits in the menu bar until the permission is granted. Its log is written to
`~/Library/Logs/ElaraWM/elara.log`; the log of the run before is kept as `elara.log.1`.

"Start at Login" in the menu needs the application: it is greyed out for `cargo run`.

macOS ties the Accessibility permission to the application's signature. The script signs
with a certificate named "ElaraWM Signing" if your keychain has one, and the permission then
carries over from one build to the next. Without it the script signs ad hoc, which needs no
certificate, but macOS then takes every build for a different application: the permission has
to be granted again after each rebuild, and the old entries pile up in the list.

To make the certificate, open Keychain Access and choose Certificate Assistant > Create a
Certificate, with the name "ElaraWM Signing", identity type "Self-Signed Root" and certificate
type "Code Signing". It only means something on your own machine. To sign with a different
identity:

```sh
ELARA_SIGN_IDENTITY="My Certificate Name" tools/bundle-app.sh
```

With Xcode installed, the icon is compiled from the layered
`resources/icons/MacOS Icons/Elara.icon`, which gives it its dark, tinted and clear
appearances. Without Xcode the script still works and uses the flat picture in `Elara Exports`
next to it.

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
- Holding `Mod` and dragging a window with the left mouse button moves it; with the right
  button, it resizes the window by the nearest edge or corner. A tiled window that is dragged
  leaves its column and lands where it is dropped. Pressing the other button while moving
  switches it between floating and tiled. While ElaraWM runs, this replaces the macOS
  behaviour of an Option-click hiding the application you were in.
- Holding `Mod` and dragging with the middle mouse button, anywhere, drags the strip of
  columns sideways or the workspaces up and down.
- Mouse buttons can be bound like keys: `MouseLeft`, `MouseRight`, `MouseMiddle`, `MouseBack`
  and `MouseForward`, as in `Mod+MouseMiddle { close-window; }`.
- The focus ring, borders, the tab indicator and the insert hint come from the `layout`
  section, as in niri. Clicking a tab switches to its window. They
  are drawn in a transparent window on top of the others, in the gaps between windows. A
  gradient is drawn as its first color. macOS does not say how round a window's corners are,
  so they are taken to have a radius of 16; set `geometry-corner-radius` in a window rule if
  the ring does not hug your windows.
- ElaraWM arranges the windows of one macOS Space: the one that is showing when it starts.
  Windows on other Spaces are left as they are, and while you are on another Space ElaraWM
  takes no keys and moves nothing. A window dragged onto its Space joins the layout, and one
  dragged off it leaves, a second or two later. Workspaces in the niri sense are ElaraWM's
  own and all live on that one Space.
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
