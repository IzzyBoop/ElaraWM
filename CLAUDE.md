# ElaraWM

A port of the niri window manager (https://github.com/niri-wm/niri, tag v26.04) to macOS 27.
Fidelity to niri is the main goal: reuse upstream Rust code wherever possible and replace only the
Wayland/Smithay backend.

## Fixed decisions

- Rust; niri's layout, config and IPC code is copied in and tracked in `UPSTREAM.md`.
- SIP stays enabled. Accessibility API only: no private SkyLight/CGS calls, no Dock injection.
- Virtual workspaces are implemented by ElaraWM inside one macOS Space per display. Native Spaces
  are not used.
- Binary is `elara`; config is `~/.config/elara/config.kdl` in niri's KDL format.
- `Mod` in binds is configurable and defaults to Option.
- niri's animation clock stays in the core. The strip of columns and the workspaces glide
  (horizontal-view-movement and workspace-switch), windows glide when they trade places or
  move between columns (window-movement), and the overview zooms (overview-open-close);
  window open, close and resize take effect at once.
  `animations { off }` turns these off too.
- The overview is a picture: a window of ElaraWM's own covers each display and draws a card
  per window where niri would draw the window zoomed out. The real windows stay under it,
  arranged as they will be when it closes. By default there is no screen capture, so no
  extra permission. `window-thumbnails` in the `overview` section, an addition to niri's
  config, has the cards show pictures of the windows, taken with ScreenCaptureKit; that
  needs the Screen Recording permission, and without it the cards stay. Four more
  additions there: `backdrop-image` (an image behind the workspaces), `backdrop-blur`,
  `window-opacity` and `workspace-corner-radius`.
- macOS lets no window go above the menu bar, so the windows of a workspace sliding in from
  above or out at the top are kept out of sight while their place is partly up there. With
  `window-thumbnails` on, a picture of each is drawn in its place (`Ghost` in `elara-macos`),
  cut off at the menu bar. Without it they are simply gone until they can be there.
- The hotkey overlay is niri's `src/ui/hotkey_overlay.rs` in the `elara` crate, deciding
  what to list; `elara-macos` draws the list with AppKit in a small window on each display.
  Its look is ElaraWM's own, not niri's plain box: window-like round corners, and around it
  the active focus ring's line (width, color or gradient) from the config.
  Pango markup in titles is read by `crates/elara/src/ui/markup.rs`, which knows a subset.
- One binary, run either with `cargo run -p elara` or from `ElaraWM.app`, which
  `tools/bundle-app.sh` builds around it. Keep macOS-facing code in `elara-macos`.
- License is GPL-3.0-or-later.

## Layout

```
crates/
  elara-compat/   stand-ins for the Smithay types niri's code expects
  niri-ipc/       imported from upstream
  niri-config/    imported from upstream
  elara-layout/   niri src/layout and src/animation, with inert render stand-ins
  elara-macos/    Accessibility, displays, event tap, permissions
  elara/          daemon binary
```

Crates are added as their stage lands; only the ones present under `crates/` exist so far.

## Rules for imported code

- Keep upstream file paths and names.
- Mark every divergence with `// ELARA: <reason>`.
- Update the file table in `UPSTREAM.md` in the same commit.
- Layout logic belongs in `elara-layout` only. `elara-macos` feeds events in and applies the
  rectangles the layout computes.
- Do not edit drawing code out of imported layout files. Rendering is neutralised by the
  stand-ins in `elara-layout/src/render_helpers` and `elara-compat`; see "How rendering is
  handled" in `UPSTREAM.md`. A compile error from a new upstream render call is fixed in the
  stand-in.

## Git workflow

- Never commit to `main` directly. One feature branch per unit of work
  (`feat/…`, `fix/…`, `chore/…`, `docs/…`, `test/…`).
- Conventional commit messages with a body explaining why.
- Merge into `main` locally with `git merge --no-ff` once the merge gate passes.

## Merge gate

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo +nightly fmt --all -- --check
```

`rustfmt.toml` is niri's and uses unstable options, so formatting needs nightly rustfmt.

After touching `elara-layout` or `elara-compat`, also run niri's slow exhaustive and
property-based layout tests (about 15 seconds in release mode):

```sh
RUN_SLOW_TESTS=1 cargo test --release -p elara-layout
```

If the pinned clippy raises a lint on unmodified upstream code, allow it in that crate's
`Cargo.toml` under `[lints.clippy]` with an `ELARA:` comment. Do not edit the upstream file.

## Checking against the real system

Window access needs the Accessibility permission, which macOS grants per application. For a
cargo build it attaches to whatever launched the binary: the terminal or the editor.

```sh
cargo run -p elara-macos --example probe            # permission, displays, windows
cargo run -p elara-macos --example probe -- --ask   # also show the system permission dialog
cargo run -p elara-macos --example thumbnails -- 4  # window pictures, shown for 4 seconds
cargo run -p elara-macos --example ghosts -- 3      # a window's picture sliding in from the top
cargo run -p elara --example hotkey_overlay -- --show 3   # the hotkey overlay, for 3 seconds
```

The `hotkey_overlay` example is not the daemon: it only draws the list, and moves nothing.

The `thumbnails` and `ghosts` examples need the Screen Recording permission as well and stop
without it. Do not pass it `--ask` unasked: that puts a system dialog on the user's screen.

**Do not run the `elara` binary, or open `ElaraWM.app`, to check something unless the user asked
for a live run.** Once
the permission is granted it takes over the real desktop at once: it resizes and moves every
window and intercepts the bound keys, for as long as it runs. The probe only reads. If `elara`
is running and should not be, stop it with `pkill -INT -x elara`; SIGINT lets it give windows
their original frames back, where SIGKILL would leave them parked off screen.

Everything above `elara-macos` is tested against `MockBackend`, which needs neither a display
nor the permission. Code in `elara-macos/src/macos/` can only be checked by running it with the
permission granted; say so plainly when a change there has not been run that way.

## Releases

`tools/release.sh` builds `ElaraWM.app` and zips it as
`target/release/ElaraWM-<version>-<arch>.zip`; the version is the one in `Cargo.toml`. Tag
the commit `v<version>` and attach the zip to a GitHub release. Always sign with the same
identity. Making a release publishes it: do it only when asked.

## Toolchain

rustup is installed with Homebrew and is keg-only. If `cargo` is not on `PATH`:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```
