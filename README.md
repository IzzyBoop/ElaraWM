<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="resources/icons/elara-white.svg">
    <img src="resources/icons/elara.svg" alt="ElaraWM" width="220">
  </picture>
</p>

# ElaraWM

A scrollable-tiling window manager for macOS, ported from
[niri](https://github.com/niri-wm/niri).

Windows are arranged in columns on a strip that scrolls sideways, on workspaces that stack
up and down. Opening a window never resizes the ones you already have. ElaraWM runs niri's
own layout and configuration code (tag v26.04); only the Wayland compositor underneath is
replaced, by a backend that moves real application windows through the Accessibility API.
System Integrity Protection stays on and no private APIs are used.

**Status:** early development, in daily use on one display. Expect rough edges, and read
[Quirks and known issues](#quirks-and-known-issues) before relying on it.

## Contents

- [Requirements](#requirements)
- [Installing and starting](#installing-and-starting)
- [Permissions](#permissions)
- [Using it](#using-it)
- [Configuration](#configuration)
- [What is different from niri](#what-is-different-from-niri)
- [Quirks and known issues](#quirks-and-known-issues)
- [Troubleshooting](#troubleshooting)
- [Not there yet](#not-there-yet)
- [Building from source](#building-from-source)
- [License](#license)

## Requirements

- macOS 27 on a Mac with Apple silicon. That is what ElaraWM is developed and tested on,
  and the download is built for Apple silicon only.
- Nothing else. Building from source is optional and is described
  [further down](#building-from-source).

## Installing and starting

1. Download `ElaraWM.app` from the Releases page of this repository and unzip it.
2. Move it to your Applications folder. Do this before opening it: macOS runs a downloaded
   application from a temporary place until it has been moved, and permissions and "Start
   at Login" do not stick there.
3. Open it. The first time, macOS refuses, because ElaraWM is not notarized by Apple. Open
   System Settings > Privacy & Security, scroll down to the message about ElaraWM and
   choose "Open Anyway". This is needed once.
4. macOS then offers to open System Settings for the Accessibility permission. Switch
   ElaraWM on there. It waits in the menu bar until you do, and starts arranging your
   windows as soon as you have.

ElaraWM has no Dock icon and no window, only an item in the menu bar. "Start at Login" in
its menu has it start with your Mac. Its log goes to `~/Library/Logs/ElaraWM/elara.log`;
the log of the run before is kept as `elara.log.1`.

The first time you open the overview (`Mod+O`), macOS asks for a second permission, Screen
Recording. See [Permissions](#permissions) for what it is for and how to do without it.

### Updating

Quit ElaraWM, replace the application in your Applications folder with the new one, and
open it. Your config is kept. If nothing is tiled after an update, macOS has taken the new
version for a different application: see [Troubleshooting](#troubleshooting).

### Uninstalling

Quit ElaraWM, which puts your windows back, and move the application to the Trash. Its
config is in `~/.config/elara` and its log in `~/Library/Logs/ElaraWM`. Remove its entries
under Accessibility and Screen Recording in System Settings > Privacy & Security.

### Stopping

Any of these stops ElaraWM and puts every window back where it was before ElaraWM took it:

- `Mod+Shift+E`, the default `quit` bind. It quits at once; niri's confirmation dialog is
  not shown.
- "Quit ElaraWM" in the menu bar item.
- `pkill -INT -x elara` in a terminal, or Ctrl-C if you started it from one.

Do not force-quit it or use `kill -9`. See [Troubleshooting](#troubleshooting) if you did.

## Permissions

| Permission | Needed for | Without it |
| --- | --- | --- |
| Accessibility | Moving and resizing windows, and seeing keys for the binds | ElaraWM does nothing and waits |
| Screen Recording | Pictures of windows in the overview and while workspaces slide | Works, with cards in place of pictures |

Screen Recording is only asked for if `window-thumbnails` is on in the config, which it is
in the config ElaraWM writes for you. macOS asks the first time the overview is opened;
once you grant it, quit ElaraWM and start it again. On recent versions of macOS the system
reminds you from time to time that ElaraWM can record the screen. The pictures are used for
nothing else and never leave ElaraWM's memory. To do without the permission, remove
`window-thumbnails` from the config.

## Using it

`Mod` is the Option key unless you change it. These are the most used of the default
binds; `Mod+Shift+/` shows a list on screen, and the config file has them all.

| Keys | What they do |
| --- | --- |
| `Mod+Left` / `Right` (or `H` / `L`) | Focus the column to the left or right |
| `Mod+Up` / `Down` (or `K` / `J`) | Focus the window above or below in a column |
| `Mod+Ctrl+` arrows | Move the column or window that way |
| `Mod+U` / `I` (or `Page_Down` / `Page_Up`) | Switch to the workspace below or above |
| `Mod+1` … `9` | Switch to a workspace by number |
| `Mod+Ctrl+1` … `9` | Move the column to that workspace |
| `Mod+,` / `Mod+.` | Pull the next window into this column, or push one out |
| `Mod+[` / `Mod+]` | Move a window into or out of the neighboring column |
| `Mod+R` | Cycle the column through the preset widths |
| `Mod+-` / `Mod+=` | Make the column narrower or wider |
| `Mod+F` | Give the column the full width |
| `Mod+Shift+F` | Fullscreen the window |
| `Mod+C` | Center the column |
| `Mod+W` | Show the column's windows as tabs |
| `Mod+V` | Switch the window between floating and tiled |
| `Mod+O` | Open or close the overview |
| `Mod+Q` | Close the window |
| `Mod+T` | Open Terminal |
| `Mod+Shift+E` | Quit ElaraWM |

### Mouse and trackpad

- **Resize a tiled window** by dragging one of its edges, with nothing held, the way macOS
  has it. Its column takes the new width and the window the new height, and the neighbors
  make room as you drag. A tiled window dragged by its title bar goes back to its place
  when you let go.
- **`Mod` + left drag** on a window moves it. A tiled window leaves its column and lands
  where you drop it; a hint shows where. Pressing the right button while moving switches
  the window between floating and tiled.
- **`Mod` + right drag** resizes a window by the nearest edge or corner.
- **`Mod` + middle drag**, anywhere, drags the strip of columns sideways or the workspaces
  up and down.
- **`Mod` + two-finger scroll** on the trackpad does the same, like niri's three-finger
  swipe.
- **`Mod` + mouse wheel** switches workspaces, and with Shift moves between columns. These
  are the `WheelScroll…` binds in the config.
- **Clicking a tab** of a tabbed column switches to its window.

### The overview

`Mod+O` shows the workspaces of each display zoomed out, with the columns that are out of
view beside them. In it:

- A click on a window or a workspace goes to it.
- Dragging a window moves it, to another workspace if you like.
- Dragging with the right button, or scrolling with two fingers, moves a workspace's
  columns sideways or the workspaces up and down.
- The mouse wheel switches workspaces, and columns with Shift held.
- The arrow keys move around; Escape or Return closes it.

None of that needs `Mod`. Other keys reach no window while the overview is open, except
with Command or Control held.

macOS does not let another application's window be drawn small, so each window is shown as
a picture of it, taken again about once a second (a video in a window moves in steps).
Without the Screen Recording permission, and for any window no picture could be taken of,
a card with the application's icon and the window's title stands in.

### The menu bar item

ElaraWM's mark, followed by the name or number of the workspace that is showing. Its menu
opens the config file, shows the log, switches starting at login on or off, and quits.

### The hotkey list

`Mod+Shift+/` shows niri's list of important hotkeys in the middle of each display.
Pressing it again, or any other key, a click or scrolling, takes it away. It also shows
when ElaraWM starts; `skip-at-startup` in the `hotkey-overlay` section stops that, and
`hide-not-bound` there leaves out the actions that have no key. A bind's
`hotkey-overlay-title` sets what its line says, or with `null` leaves the bind out. Of the
Pango markup niri allows in a title, `<b>`, `<i>`, `<tt>` and the font, weight, style and
color attributes of `<span>` have an effect.

## Configuration

ElaraWM reads `~/.config/elara/config.kdl`, in
[niri's format](https://niri-wm.github.io/niri/Configuration:-Introduction). niri's
documentation applies, apart from what is listed on this page. An existing niri config can
be copied over as it is: what does not apply on macOS is skipped, not rejected.

- The file is created on first start from niri's default config, adjusted for macOS. Every
  changed line is marked with `// ElaraWM:` and says why.
- It is reloaded whenever it changes. If an edit has an error, the error is logged and the
  config from before stays in use.
- "Open Config" in the menu bar item opens it.

### What means something else on macOS

- **`Mod`** is Option. Set `mod-key` in the `input` section to change it. In binds, `Super`
  is Command and `Alt` is Option.
- **`app-id`** in window rules is matched against the application's bundle identifier, such
  as `com.apple.Terminal` or `com.apple.Safari`. To find an application's identifier, run
  `osascript -e 'id of app "Safari"'` in a terminal with its name.
- **Output names** are display names, such as `"Built-in Retina Display"`.
- **Mouse buttons** can be bound like keys: `MouseLeft`, `MouseRight`, `MouseMiddle`,
  `MouseBack` and `MouseForward`, as in `Mod+MouseMiddle { close-window; }`.
- **`geometry-corner-radius`**: macOS does not say how round a window's corners are, so
  they are taken to have a radius of 16. Set this in a window rule if the focus ring does
  not hug a window.
- **`fullscreen-window`** makes the window fill the display up to the menu bar, which macOS
  keeps on screen. It is not the macOS full screen of the green button.

### What ElaraWM adds

One setting, in the `overview` section:

```kdl
overview {
    window-thumbnails
}
```

It has the overview show pictures of the windows in place of cards, and has a workspace
that slides in from above show pictures of its windows on the way (see
[The menu bar is in the way](#the-menu-bar-is-in-the-way)). It needs the Screen Recording
permission. The config ElaraWM writes has it on; a config without it has it off.

### What is read and has no effect

These are accepted, so that a niri config loads, and then ignored. The sections in the
first four points are named in the log when a config sets them.

- `output`: macOS sets display modes, scale and arrangement.
- `input` > `keyboard`, `touchpad`, `mouse`, `trackpoint`: macOS sets the keyboard layout,
  key repeat, tap-to-click, scroll direction and pointer speed, in System Settings.
- `cursor`: macOS draws the pointer.
- `layer-rule`, `switch-events`, `xwayland-satellite`: these have no counterpart.
- Shadows, in the `layout` section and in window rules. macOS gives windows their own.
- Window rules that change how a window is drawn, such as `opacity`, `block-out-from` and
  `clip-to-geometry`. ElaraWM cannot draw other applications' windows.
- Animation settings for windows opening, closing and resizing. Those take effect at once.
- `focus-follows-mouse` and `warp-mouse-to-focus`.

Actions that have no counterpart do nothing and leave a line in the log: the screenshot
actions (macOS has Shift+Command+3, 4 and 5), screencasts, the recent-windows switcher
(Command+Tab is still there), switching keyboard layouts, `power-off-monitors` and
`toggle-keyboard-shortcuts-inhibit`.

## What is different from niri

ElaraWM is a window manager, not a compositor. macOS draws every window and decides a good
deal about where it may go. What follows from that:

### Windows are moved, not drawn

ElaraWM asks each application to move and resize its windows. So:

- It cannot change another application's shadow, opacity or corner radius, show a window
  smaller than it is, or blur what is behind one.
- The focus ring, borders, the tab indicator and the insert hint are drawn in a transparent
  window of ElaraWM's own, on top of the others, in the gaps the layout leaves. They follow
  niri's `layout` settings, gradients included.
- An application decides how fast its window moves. Most keep up with the animations; a
  busy or slow one lags behind its focus ring for a moment.
- An application can refuse a size. Terminals snap to whole rows and columns, some windows
  have a minimum size, some cannot be resized at all. The layout is told what the window
  took and arranges the rest around it, as niri does for clients that pick their own size.
  Windows that cannot be resized, and dialogs, float by default.

### The menu bar is in the way

macOS lets no window go above the menu bar. A workspace that slides in from above, or out
at the top, has its windows kept out of sight while their place is partly up there. With
`window-thumbnails` on, a picture of each slides in its place and the window takes over
when it can be there itself. The first time a window is needed like this, its picture can
take a moment to turn up, and the picture has no focus ring or border. Without
`window-thumbnails` such windows are simply gone for that part of the way.

### Workspaces are ElaraWM's own

niri's workspaces all live on one macOS Space: the one that is showing when ElaraWM
starts. macOS cannot hide another application's window, so windows on other workspaces,
and columns scrolled out of view, are parked past a corner of the display with a single
pixel still on it.

- Windows on other macOS Spaces are left as they are. While you are on another Space,
  ElaraWM takes no keys and moves nothing.
- A window dragged onto ElaraWM's Space joins the layout, and one dragged off it leaves, a
  second or two later.
- A minimized window, and one in macOS full screen, is out of the layout until it is back.

### Gestures and corners

niri's three-finger swipes, four-finger swipe and hot corner are not there: macOS keeps
those for itself. `Mod` + two-finger scroll stands in for the swipes.

### Animations

The strip of columns, the workspaces, windows trading places and the overview glide as in
niri. Windows opening, closing and resizing take effect at once. `animations { off }` turns
all of it off.

## Quirks and known issues

- **Option stops typing special characters on bound keys.** A bind on `Mod+<key>` takes
  over Option+that key, which macOS otherwise uses to type special characters. Remove the
  bind to get the character back, or set `mod-key` to another modifier.
- **Option-click no longer hides the application you were in.** While ElaraWM runs, `Mod` +
  click belongs to moving windows.
- **Binds do not work while a password field has the keyboard.** macOS hides the keys from
  every other program then. The same goes for a terminal with "Secure Keyboard Entry" on,
  for as long as it has focus.
- **Mission Control shows every window, parked ones included,** since they are all on one
  Space. The focus ring and borders hide while it is open.
- **Command+Tab and the Dock work as always.** Going to an application whose window is on
  another workspace switches to that workspace.
- **A sliver of a parked window** can show as one pixel in a corner of the display.
- **With "Displays have separate Spaces" off** (System Settings > Desktop & Dock), the
  displays form one desktop, and a window parked at the edge of one display can show on
  the next. ElaraWM says so in its log at startup. Leave the setting on.
- **A window that opens at a size of its own** can show at that size for a moment before
  it is placed.
- **Stage Manager** has not been tried with ElaraWM.
- **More than one display** is supported in the code (the `…-monitor-…` actions,
  workspaces per display, plugging and unplugging) but has had almost no real use.
- **The system's reminder about screen recording** comes back now and then if
  `window-thumbnails` is on. That is macOS, and cannot be switched off from here.

## Troubleshooting

**macOS will not open it.** A downloaded ElaraWM is not notarized by Apple. Open System
Settings > Privacy & Security, scroll down to the message about ElaraWM and choose "Open
Anyway". If there is no such message, this does the same in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/ElaraWM.app
```

**Nothing is tiled after starting.** The Accessibility permission is missing. Look for
ElaraWM in System Settings > Privacy & Security > Accessibility and switch it on. If it is
listed and on but nothing happens, the entry belongs to an older version: remove it with
the minus button, start ElaraWM and grant it again. The same goes for Screen Recording.

**The overview shows cards, not pictures.** Screen Recording is missing, or was granted
while ElaraWM was running. Grant it, then quit and start ElaraWM.

**Windows are stuck off screen.** ElaraWM was killed or crashed before it could put them
back. Start it again and quit it the ordinary way; that brings every window back on
screen.

**A key does nothing, or does the wrong thing.** Look at the log ("Show Log" in the menu). A bind for an action that is not supported is named there, and so is a
config that failed to load, in which case the config from before is still in use.

**See what ElaraWM sees, without it managing anything.** This needs the source and
rustup (see [Building from source](#building-from-source)):

```sh
cargo run -p elara-macos --example probe
```

It prints whether the permission is granted, the displays, and every window with its
application's bundle identifier, which is what `app-id` in window rules matches.

**Reporting a problem.** Include the log ("Show Log" in the menu), the macOS version and
the application whose window misbehaved.

## Not there yet

- `elara msg` and the IPC socket, niri's way of scripting the window manager.
- Notarization, so that macOS opens a downloaded ElaraWM without being told to.
- A build for Intel Macs, and a try on versions of macOS older than 27.
- Proper testing on more than one display.

Not planned, since macOS has its own: screenshots and window shadows.

[ROADMAP.md](ROADMAP.md) has what is planned and in what order.

## Building from source

You only need this to change ElaraWM or to run a version that has not been released.
It takes [rustup](https://rustup.rs); the compiler version is pinned in
`rust-toolchain.toml` and rustup fetches it by itself.

### The application

```sh
tools/bundle-app.sh                 # writes target/release/ElaraWM.app
tools/bundle-app.sh /Applications   # or puts it somewhere else
```

With Xcode installed the application gets its layered icon, with dark, tinted and clear
appearances; without it, a flat one.

**Sign it with a certificate of your own, or you will grant the permissions again after
every rebuild.** macOS ties permissions to the application's signature. The script signs
with a certificate named "ElaraWM Signing" if your keychain has one, and the permissions
then carry over from build to build. Without it the script signs ad hoc, macOS takes every
build for a different application, and the old entries pile up in System Settings.

To make the certificate: open Keychain Access, choose Certificate Assistant > Create a
Certificate, and give it the name "ElaraWM Signing", identity type "Self-Signed Root" and
certificate type "Code Signing". It only means something on your own machine. To sign with
a different identity:

```sh
ELARA_SIGN_IDENTITY="My Certificate Name" tools/bundle-app.sh
```

### From a terminal

```sh
cargo run -p elara
```

Run this way, the permissions belong to the terminal (or the editor whose terminal you
use), not to ElaraWM. The log is printed to the terminal; set `RUST_LOG=elara=debug` for
more of it. `elara --config <path>` reads a config from somewhere else. "Start at Login"
in the menu is greyed out, since there is no application to start. Stop it with Ctrl-C.

### Developing

```sh
cargo build
cargo test --workspace
```

The tests need neither a display nor any permission. `CLAUDE.md` describes the layout of
the code, the rules for code imported from niri, and the checks a change has to pass;
`UPSTREAM.md` lists every imported file and how it differs from niri's.

## License

GPL-3.0-or-later, the same as niri. See [LICENSE](LICENSE) and [UPSTREAM.md](UPSTREAM.md).
