<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="resources/icons/elara-white.svg">
    <img src="resources/icons/elara.svg" alt="ElaraWM" width="220">
  </picture>
</p>

# ElaraWM

A scrollable-tiling window manager for macOS, ported from
[niri](https://github.com/niri-wm/niri).

Windows sit in columns on a strip that scrolls sideways, and workspaces stack vertically.
Opening a new window never resizes the ones you already have.

ElaraWM runs niri's own layout and configuration code. It works with System Integrity
Protection enabled and uses no private APIs.

> **Early beta.** ElaraWM is in daily use on one Mac with a single display, and has been
> tested nowhere else. Expect rough edges.

## Features

- **Scrolling columns and dynamic workspaces**, with the same behavior as niri.
- **niri's config format.** An existing niri `config.kdl` works as is, and changes are
  picked up as soon as you save.
- **Focus ring, borders and tab indicators**, with gradient support.
- **An overview** of all your workspaces, with live window previews and an optional
  background image.
- **Smooth animations** when you scroll, switch workspaces or rearrange windows.
- **Mouse support**: drag a window's edge to resize it, or hold Option and drag to move it.
- **A menu bar item** that shows the current workspace, with an option to start at login.

## Installation

Requires macOS 27 on an Apple silicon Mac.

1. Download `ElaraWM.app` from the
   [Releases page](https://github.com/IzzyBoop/ElaraWM/releases) and unzip it.
2. Move it to your Applications folder before opening it.
3. Open it. macOS blocks it the first time because ElaraWM isn't notarized by Apple yet.
   Go to System Settings > Privacy & Security, scroll down to the message about ElaraWM,
   and click "Open Anyway". You only need to do this once.
4. Grant the Accessibility permission when macOS asks. ElaraWM needs it to move windows.

ElaraWM then appears in the menu bar and starts arranging your windows. It has no Dock
icon and no window of its own.

The first time you open the overview, macOS also asks for the Screen Recording
permission, which ElaraWM uses for window previews. This one is optional.

To update, quit ElaraWM and replace the app with the new version. To uninstall, quit it
and move the app to the Trash; quitting puts your windows back where they were.

## Getting started

`Mod` is the Option key. Press `Mod+Shift+/` at any time to see the main shortcuts.

| Keys | Action |
| --- | --- |
| `Mod+Left` / `Right` | Focus the column to the left or right |
| `Mod+Up` / `Down` | Focus the window above or below |
| `Mod+Ctrl+` arrows | Move the column or window |
| `Mod+U` / `I` | Switch to the workspace below or above |
| `Mod+1` … `9` | Switch to a workspace by number |
| `Mod+R` | Cycle through column widths |
| `Mod+F` | Make the column full width |
| `Mod+V` | Toggle floating |
| `Mod+O` | Open the overview |
| `Mod+Q` | Close the window |
| `Mod+Shift+E` | Quit ElaraWM |

Always quit ElaraWM with `Mod+Shift+E` or from its menu bar item rather than force-quitting
it, so that it can restore your windows.

## Configuration

ElaraWM reads `~/.config/elara/config.kdl`, which it creates on first launch. The file uses
[niri's format](https://niri-wm.github.io/niri/Configuration:-Introduction), so niri's
documentation applies. Settings that only make sense on Linux are accepted and ignored.

A few things differ on macOS. For example, `app-id` in window rules matches an
application's bundle identifier, such as `com.apple.Safari`. The
[guide](docs/GUIDE.md#configuration) covers all of the differences.

## Limitations

ElaraWM is a window manager, not a compositor like niri: macOS draws the windows, and
ElaraWM can only move and resize them. As a result:

- It can't change other applications' shadows, opacity or corner radius.
- Opening, closing and resizing windows aren't animated.
- A bind on `Mod+<key>` replaces the special character that Option+key normally types.
- Shortcuts don't work while a password field is focused.
- Multiple displays are supported but barely tested.

## Documentation

- [Guide](docs/GUIDE.md): every shortcut and mouse action, configuration details, how
  ElaraWM differs from niri and why, known quirks, troubleshooting and privacy.
- [Changelog](CHANGELOG.md): what changed in each release.
- [Roadmap](ROADMAP.md): what's planned.
- [Upstream](UPSTREAM.md): which code comes from niri and how it differs.

## Building from source

You only need to build ElaraWM yourself if you want to change it or run an unreleased
version. Install [rustup](https://rustup.rs); it fetches the pinned compiler automatically.

Build the app:

```sh
tools/bundle-app.sh                 # creates target/release/ElaraWM.app
tools/bundle-app.sh /Applications   # or installs it somewhere else
```

Or run it straight from a terminal. In this case the permissions belong to your terminal,
and Ctrl-C quits:

```sh
cargo run -p elara
```

Run the tests, which need no display and no permissions:

```sh
cargo test --workspace
```

**Sign your builds with your own certificate.** macOS ties permissions to an app's
signature. Without a certificate, every rebuild counts as a new app and you have to grant
the permissions again. The build script uses a certificate named "ElaraWM Signing" if your
keychain has one. To create it, open Keychain Access and choose Certificate Assistant >
Create a Certificate, then enter the name "ElaraWM Signing", identity type "Self-Signed
Root", and certificate type "Code Signing". To use a different certificate:

```sh
ELARA_SIGN_IDENTITY="My Certificate Name" tools/bundle-app.sh
```

With Xcode installed, the app gets its layered icon with dark and tinted variants. Without
Xcode, it uses a flat icon.

`CLAUDE.md` describes the code layout and the checks every change has to pass.

## License

GPL-3.0-or-later, the same as niri. See [LICENSE](LICENSE) and [UPSTREAM.md](UPSTREAM.md).
