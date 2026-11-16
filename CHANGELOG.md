# Changelog

All notable changes to ElaraWM are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). While ElaraWM is at 0.x, any release may change
behavior.

## [Unreleased]

### Fixed

- **QuickTime Player windows got stuck when tiled.** After being resized, most easily by
  dragging an edge, a QuickTime window could be made smaller but never larger again. A
  window that keeps the shape of its video does not take the size it is asked for, and
  ElaraWM mistook its answers for size limits. It now recognizes such windows, only asks
  them for sizes of their own shape, and lets their height follow their width, so they
  fit their column and resize with it in both directions.
- A window that had once settled on a size of its own could ignore a later request for
  that same size.

## [0.2.0] - 2026-11-16

This release is about the overview: it can now sit on your wallpaper or an image of your
choice, and its workspaces and window previews can be styled to match.

### Added

- **Wallpaper backdrop.** `backdrop-wallpaper` in the `overview` section shows your
  current macOS wallpaper behind the workspaces. Each display uses its own wallpaper, a
  change is picked up the next time the overview opens, and no extra permission is needed.
- **Image backdrop.** `backdrop-image "<path>"` shows an image of your choice instead. Any
  format macOS can open works, and the path may start with `~/`. The image is scaled to
  fill each display. If both settings are present, `backdrop-image` wins.
- **Backdrop blur.** `backdrop-blur <points>` blurs either backdrop. The image is blurred
  once per display and cached, so it doesn't slow down the overview's animation.
- **Window opacity.** `window-opacity <0 to 1>` makes window previews and cards in the
  overview partly see-through.
- **Rounded workspaces.** `workspace-corner-radius <points>` rounds the corners of the
  workspaces in the overview. The corners are square while a workspace fills the display
  and round off as the overview zooms out.
- **A user guide.** [docs/GUIDE.md](docs/GUIDE.md) covers permissions, every shortcut and
  mouse action, configuration details, how ElaraWM differs from niri, known quirks,
  troubleshooting and privacy.
- The `probe` example prints each display's wallpaper file, to show what macOS reports.

### Changed

- The README is now a short overview: what ElaraWM is, its features, installation and
  building from source. The detail moved to the guide.
- Workspace backgrounds in the overview keep the opacity of niri's `background-color`
  setting, so a color such as `"#25273a80"` lets the backdrop show through. This already
  worked; it is now tested and documented.

### Notes

- A wallpaper that changes through the day is shown as its first image. For video
  wallpapers or a solid color, macOS may not report a picture; the backdrop color is shown
  in that case.
- Window previews are not clipped to rounded workspace corners. With a radius close to
  your gap size nothing overlaps; a much larger one lets window corners reach past the
  curve.

## [0.1.0] - 2026-11-15

The first release. An early beta, in daily use on one Mac with a single display.

### Added

- **niri's layout on macOS.** Scrolling columns and dynamic workspaces, driven by niri's
  own layout code (tag v26.04). Windows are moved and resized through the Accessibility
  API, with System Integrity Protection enabled and no private APIs.
- **niri's configuration.** `~/.config/elara/config.kdl` uses niri's KDL format, is created
  on first launch from niri's default config adapted for macOS, and reloads when saved.
  Settings that only apply on Linux are accepted and ignored.
- **Keyboard shortcuts** in niri's bind syntax, with `Mod` as the Option key by default,
  and mouse buttons bindable like keys.
- **Mouse and trackpad control.** `Mod`+drag to move and resize windows, `Mod`+scroll and
  `Mod`+middle-drag to scroll through columns and workspaces, and resizing a tiled window
  by dragging its edge with no modifier.
- **Tabbed columns, floating windows, fullscreen and maximize**, as in niri.
- **Window rules**, matching `app-id` against the application's bundle identifier.
- **Focus ring, borders, tab indicator and insert hint**, drawn in the gaps between
  windows, including gradients with `angle`, `relative-to` and the `srgb`,
  `srgb-linear`, `oklab` and `oklch` color spaces.
- **Animations** for scrolling between columns, switching workspaces, windows changing
  places, and the overview.
- **The overview** (`Mod+O`), showing every workspace zoomed out, with mouse, trackpad and
  keyboard navigation. With `window-thumbnails` and the Screen Recording permission it
  shows live window previews; otherwise a card with each application's icon and title.
- **Window previews during workspace switches**, standing in for windows that would have
  to pass above the menu bar, where macOS allows no window.
- **The hotkey list** (`Mod+Shift+/`), styled to match the windows around it, with the
  focus ring's width, color or gradient as its border.
- **A menu bar item** showing the current workspace, with "Open Config", "Show Log",
  "Start at Login" and "Quit".
- **`ElaraWM.app`**, built by `tools/bundle-app.sh` and packaged by `tools/release.sh`.
- **Multiple displays**: workspaces per display and the monitor actions. Barely tested.

### Security and privacy

- ElaraWM makes no network connections.
- Window titles and typed text are never written to the log, at any log level.
- The log file can only be read by your own account.
- Release builds contain no paths, user names or identifiers from the machine that built
  them, and the release script refuses to package a build that does.

### Known limitations

- Not notarized: macOS needs "Open Anyway" in System Settings the first time.
- Apple silicon only, and only tested on macOS 27.
- No `elara msg` or IPC socket yet.
- Opening, closing and resizing windows aren't animated.
- Other applications' shadows, opacity and corner radius can't be changed.

[0.2.0]: https://github.com/IzzyBoop/ElaraWM/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/IzzyBoop/ElaraWM/releases/tag/v0.1.0
