# ElaraWM guide

The details behind the [README](../README.md): permissions, every way to drive ElaraWM,
how its configuration differs from niri's, why some things work the way they do on macOS,
known quirks, and troubleshooting.

## Contents

- [Permissions](#permissions)
- [Keys](#keys)
- [Mouse and trackpad](#mouse-and-trackpad)
- [The overview](#the-overview)
- [The menu bar item](#the-menu-bar-item)
- [The hotkey list](#the-hotkey-list)
- [Configuration](#configuration)
- [How ElaraWM differs from niri](#how-elarawm-differs-from-niri)
- [Quirks and known issues](#quirks-and-known-issues)
- [Troubleshooting](#troubleshooting)
- [Privacy](#privacy)

## Permissions

| Permission | Used for | Without it |
| --- | --- | --- |
| Accessibility | Moving and resizing windows, and detecting keyboard shortcuts | ElaraWM waits and does nothing |
| Screen Recording | Window previews in the overview and during workspace switches | Everything works, with placeholder cards instead of previews |

ElaraWM only asks for Screen Recording if `window-thumbnails` is enabled in the config,
which it is by default. macOS prompts the first time you open the overview. After granting
it, quit ElaraWM and start it again.

Recent versions of macOS periodically remind you that ElaraWM can record the screen. That
reminder comes from macOS and can't be turned off by ElaraWM. To avoid the permission
entirely, remove `window-thumbnails` from the config.

## Keys

`Mod` is the Option key by default. These are the most useful default shortcuts. Press
`Mod+Shift+/` to see a list on screen; the config file contains all of them.

| Keys | Action |
| --- | --- |
| `Mod+Left` / `Right` (or `H` / `L`) | Focus the column to the left or right |
| `Mod+Up` / `Down` (or `K` / `J`) | Focus the window above or below |
| `Mod+Ctrl+` arrows | Move the column or window in that direction |
| `Mod+U` / `I` (or `Page_Down` / `Page_Up`) | Switch to the workspace below or above |
| `Mod+1` … `9` | Switch to a workspace by number |
| `Mod+Ctrl+1` … `9` | Move the column to that workspace |
| `Mod+,` / `Mod+.` | Pull the next window into this column, or push one out |
| `Mod+[` / `Mod+]` | Move a window into or out of the neighboring column |
| `Mod+R` | Cycle through the preset column widths |
| `Mod+-` / `Mod+=` | Shrink or grow the column |
| `Mod+F` | Make the column full width |
| `Mod+Shift+F` | Make the window fullscreen |
| `Mod+C` | Center the column |
| `Mod+W` | Show the column's windows as tabs |
| `Mod+V` | Toggle the window between floating and tiled |
| `Mod+O` | Toggle the overview |
| `Mod+Q` | Close the window |
| `Mod+T` | Open Terminal |
| `Mod+Shift+E` | Quit ElaraWM |

### Quitting

Each of these quits ElaraWM and restores every window to where it was before ElaraWM
started:

- `Mod+Shift+E`. It quits immediately, without niri's confirmation dialog.
- "Quit ElaraWM" in the menu bar item.
- `pkill -INT -x elara` in a terminal, or Ctrl-C if you started it from one.

Don't force-quit ElaraWM or use `kill -9`. If you did, see
[Troubleshooting](#troubleshooting).

## Mouse and trackpad

- **Drag a window's edge** to resize a tiled window, just as you normally would on macOS.
  The column takes the new width, the window takes the new height, and neighboring windows
  adjust as you drag. Dragging a tiled window by its title bar does nothing lasting: it
  snaps back when you let go.
- **`Mod` + left-drag** moves a window. A tiled window leaves its column and lands where
  you drop it, and a hint shows where that will be. Right-click while dragging to toggle
  the window between floating and tiled.
- **`Mod` + right-drag** resizes a window from the nearest edge or corner.
- **`Mod` + middle-drag**, anywhere, scrolls through columns or workspaces.
- **`Mod` + two-finger scroll** on the trackpad does the same. It replaces niri's
  three-finger swipe.
- **`Mod` + mouse wheel** switches workspaces; add Shift to move between columns. These
  are the `WheelScroll…` binds in the config.
- **Click a tab** in a tabbed column to switch to that window.

## The overview

`Mod+O` zooms out to show every workspace on each display, including columns that are
scrolled out of view.

- Click a window or workspace to go to it.
- Drag a window to move it, including to another workspace.
- Right-drag, or scroll with two fingers, to scroll through columns or workspaces.
- The mouse wheel switches workspaces; hold Shift to switch columns.
- The arrow keys move the focus. Escape or Return closes the overview.

None of this needs `Mod`. While the overview is open, other keys don't reach your windows
unless you hold Command or Control.

Each window is shown as a preview image that refreshes about once a second, so video
appears to move in steps. Without the Screen Recording permission, or when a preview can't
be captured, you see a card with the application's icon and the window's title instead.

## The menu bar item

The menu bar shows ElaraWM's icon followed by the name or number of the current workspace.
Its menu lets you open the config file, view the log, toggle "Start at Login", and quit.

## The hotkey list

`Mod+Shift+/` shows a list of important shortcuts in the middle of each display. Its
border matches your focus ring: the same width and the same color or gradient. Press the
shortcut again, press any other key, click, or scroll to dismiss it.

The list also appears when ElaraWM starts. In the `hotkey-overlay` section of the config:

- `skip-at-startup` stops it from appearing at startup.
- `hide-not-bound` hides actions that have no shortcut.

A bind's `hotkey-overlay-title` sets the text of its line, and `null` hides the bind. niri
allows Pango markup in titles; ElaraWM supports `<b>`, `<i>`, `<tt>`, and the font, weight,
style and color attributes of `<span>`.

## Configuration

ElaraWM reads `~/.config/elara/config.kdl`, which uses
[niri's format](https://niri-wm.github.io/niri/Configuration:-Introduction). niri's
documentation applies except where this page says otherwise. You can copy an existing niri
config over unchanged: settings that don't apply on macOS are ignored rather than rejected.

- The file is created on first launch from niri's default config, adapted for macOS. Every
  changed line is marked with `// ElaraWM:` and an explanation.
- The config reloads automatically when you save it. If it contains an error, the error is
  logged and the previous config stays active.
- "Open Config" in the menu bar item opens the file.

### Differences from niri

- **`Mod`** is Option. Change it with `mod-key` in the `input` section. In binds, `Super`
  means Command and `Alt` means Option.
- **`app-id`** in window rules matches the application's bundle identifier, such as
  `com.apple.Terminal` or `com.apple.Safari`. To find one, run
  `osascript -e 'id of app "Safari"'` in a terminal with the application's name.
- **Output names** are display names, such as `"Built-in Retina Display"`.
- **Mouse buttons** can be bound like keys: `MouseLeft`, `MouseRight`, `MouseMiddle`,
  `MouseBack` and `MouseForward`. For example: `Mod+MouseMiddle { close-window; }`.
- **`geometry-corner-radius`** defaults to 16, because macOS doesn't report how rounded a
  window's corners are. Set it in a window rule if the focus ring doesn't fit a window.
- **`fullscreen-window`** fills the display below the menu bar, which macOS always keeps
  visible. It isn't the same as macOS's own full screen mode (the green button).
- **Gradients** work as in niri, including `angle`, `relative-to` and the color spaces
  available through `in`.

### Added by ElaraWM

Six settings, all in the `overview` section:

```kdl
overview {
    window-thumbnails
    backdrop-wallpaper
    // backdrop-image "~/Pictures/wallpaper.jpg"
    backdrop-blur 20
    window-opacity 0.9
    workspace-corner-radius 16
}
```

- **`window-thumbnails`** shows window previews in the overview instead of cards, and
  during workspace switches (see [The menu bar limit](#the-menu-bar-limit)). It requires
  the Screen Recording permission. It's enabled in the config ElaraWM creates and disabled
  if you leave it out.
- **`backdrop-wallpaper`** shows your current macOS wallpaper behind the workspaces, in
  place of `backdrop-color`. Each display uses its own wallpaper, and a change of wallpaper
  is picked up the next time you open the overview. It needs no extra permission. Two
  limits: a wallpaper that changes through the day is shown as its first image, and for
  video wallpapers or a solid color macOS may not report a picture at all, in which case
  the backdrop color is shown.
- **`backdrop-image`** shows an image of your choice instead. If both are set, this wins.
  The image is scaled to fill each display and cropped where its shape differs. Any format
  macOS can open works, including JPEG, PNG and HEIC. Use a full path, or one starting
  with `~/`. If the file can't be read, the backdrop color is shown instead.
- **`backdrop-blur`** blurs the backdrop, whether it's your wallpaper or an image. The value is the blur radius in points;
  `0`, the default, leaves the image sharp.
- **`window-opacity`** sets how solid windows appear in the overview, from `0` (invisible)
  to `1` (the default, fully solid).

- **`workspace-corner-radius`** rounds the corners of the workspaces in the overview. The
  value is the radius in points when the overview is fully open; the corners are square
  while a workspace fills the display, and round off as the overview zooms out. The
  default is `0`.

The backdrop is only visible around the workspaces, because each workspace has its own
background. To let the image show through the workspaces as well, give that background an
opacity with niri's own `background-color` setting. It accepts any CSS color, so the last
two digits of an eight-digit hex color set the opacity:

```kdl
layout {
    background-color "#25273a80"    // about half see-through
    // background-color "transparent"   // or fully see-through
}
```

### Settings that have no effect

These are accepted so that niri configs load, but they do nothing. ElaraWM logs a note when
your config uses one of the first four.

- `output`: macOS controls display modes, scaling and arrangement.
- `input` > `keyboard`, `touchpad`, `mouse`, `trackpoint`: macOS controls the keyboard
  layout, key repeat, tap-to-click, scroll direction and pointer speed in System Settings.
- `cursor`: macOS draws the pointer.
- `layer-rule`, `switch-events`, `xwayland-satellite`: these have no macOS equivalent.
- Shadows, in the `layout` section and in window rules. macOS draws its own.
- Window rules that change how a window is drawn, such as `opacity`, `block-out-from` and
  `clip-to-geometry`.
- Animation settings for opening, closing and resizing windows. Those happen instantly.
- `focus-follows-mouse` and `warp-mouse-to-focus`.

Unsupported actions do nothing and are noted in the log: the screenshot actions (use
Shift+Command+3, 4 or 5), screencasts, the recent-windows switcher (use Command+Tab),
keyboard layout switching, `power-off-monitors` and `toggle-keyboard-shortcuts-inhibit`.

## How ElaraWM differs from niri

niri is a compositor: it draws every window itself. ElaraWM can't do that on macOS, where
the system draws all windows and controls much of where they can go. ElaraWM is a window
manager that asks applications to move and resize their own windows. Most differences from
niri follow from that.

### Windows are moved, not drawn

- ElaraWM can't change another application's shadow, opacity or corner radius, show a
  window smaller than it really is, or blur what's behind it.
- The focus ring, borders, tab indicator and insert hint are drawn in a transparent overlay
  window, in the gaps between your windows. They follow niri's `layout` settings.
- Each application decides how quickly its window moves. Most keep up with the animations,
  but a busy application's window can briefly lag behind its focus ring.
- An application can refuse a size. Terminals snap to whole rows and columns, some windows
  have a minimum size, video players such as QuickTime Player keep the shape of their
  video, and some windows can't be resized at all. ElaraWM arranges the other windows
  around whatever size the window actually takes. Windows that can't be resized, and
  dialogs, float by default.

### The menu bar limit

macOS doesn't allow any window above the menu bar. When a workspace slides in from above
or out through the top, its windows would need to pass through that area, so ElaraWM hides
them for that part of the animation.

With `window-thumbnails` enabled, a preview image of each window slides in its place and
the real window takes over once it fits on screen. The first time a window is shown this
way its preview may take a moment to appear, and the preview has no focus ring or border.
Without `window-thumbnails`, those windows are simply invisible for that part of the
animation.

### Workspaces

All of ElaraWM's workspaces live on a single macOS Space: the one that's active when
ElaraWM starts. macOS can't hide another application's window, so windows on other
workspaces, and columns scrolled out of view, are parked just past a corner of the display
with one pixel still on screen.

- Windows on other macOS Spaces are left alone. While you're on another Space, ElaraWM
  ignores its shortcuts and doesn't move anything.
- A window dragged onto ElaraWM's Space joins the layout a second or two later. A window
  dragged off it leaves the layout.
- Minimized windows and windows in macOS full screen leave the layout until they return.

### Gestures and hot corners

niri's three- and four-finger swipes and its hot corner aren't available, because macOS
reserves those. `Mod` + two-finger scroll replaces the swipes.

### Animations

Columns, workspaces, windows changing places, and the overview all animate as in niri.
Opening, closing and resizing windows happen instantly. `animations { off }` disables
everything.

## Quirks and known issues

- **Option no longer types special characters on bound keys.** A bind on `Mod+<key>` takes
  over Option+that key, which macOS normally uses for special characters. Remove the bind
  to get the character back, or set `mod-key` to a different modifier.
- **Option-click no longer hides the application you were in.** While ElaraWM runs,
  `Mod`+click is used for moving windows.
- **Shortcuts don't work while a password field is focused.** macOS hides keystrokes from
  all other programs in that case. The same applies to a terminal with "Secure Keyboard
  Entry" turned on while it has focus.
- **Mission Control shows every window, including parked ones,** because they're all on
  one Space. The focus ring and borders are hidden while it's open.
- **Command+Tab and the Dock work normally.** Switching to an application whose window is
  on another workspace takes you to that workspace.
- **A parked window can show as a single pixel** in a corner of the display.
- **Keep "Displays have separate Spaces" turned on** (System Settings > Desktop & Dock).
  With it off, a window parked at the edge of one display can appear on the next. ElaraWM
  notes this in its log at startup.
- **A window that opens at its own size** may briefly appear at that size before it's
  placed.
- **Video windows keep their shape.** A QuickTime Player window is as wide as its column
  and as tall as its video's shape allows, which usually leaves empty space below it.
  Resizing the column resizes the video with it.
- **Stage Manager** hasn't been tested with ElaraWM.
- **Multiple displays** are supported in the code (the monitor actions, per-display
  workspaces, plugging and unplugging) but have had very little real-world use.

## Troubleshooting

**macOS won't open ElaraWM.** A downloaded copy isn't notarized by Apple. Open System
Settings > Privacy & Security, scroll down to the message about ElaraWM, and click "Open
Anyway". If there's no such message, run this in a terminal instead:

```sh
xattr -dr com.apple.quarantine /Applications/ElaraWM.app
```

**Nothing is tiled after launch.** The Accessibility permission is missing. Find ElaraWM
in System Settings > Privacy & Security > Accessibility and turn it on. If it's already
listed and enabled but nothing happens, the entry belongs to an older version: remove it
with the minus button, start ElaraWM, and grant the permission again. The same applies to
Screen Recording.

**The overview shows cards instead of previews.** Screen Recording is missing, or it was
granted while ElaraWM was running. Grant it, then quit and restart ElaraWM.

**Windows are stuck off screen.** ElaraWM was killed or crashed before it could restore
them. Start it again and quit it normally; that brings every window back.

**A shortcut does nothing, or does the wrong thing.** Check the log ("Show Log" in the
menu). It names any bind that uses an unsupported action, and reports config errors. If
the config failed to load, the previous one is still in use.

**See what ElaraWM sees, without it managing anything.** This requires the source and
rustup (see [Building from source](../README.md#building-from-source)):

```sh
cargo run -p elara-macos --example probe
```

It prints whether the permission is granted, your displays, and every window along with
its application's bundle identifier.

**Reporting a problem.** Include the log, your macOS version, and the application whose
window misbehaved. Read the log before posting it: it contains the path to your config
file, which includes your user name. It never contains window titles or text you typed.
With `RUST_LOG=elara=debug`, it also lists the applications that have windows open.

## Privacy

ElaraWM makes no network connections and collects nothing. It reads the positions, sizes
and titles of your windows in order to arrange them and label the overview. With
`window-thumbnails` enabled, it also captures preview images of them. All of that stays in
memory and is discarded when ElaraWM quits.

The only things ElaraWM writes to disk are its config file and its log
(`~/Library/Logs/ElaraWM/elara.log`), which only your account can read.
