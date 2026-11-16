# Contributing to ElaraWM

ElaraWM is a port of [niri](https://github.com/niri-wm/niri) (tag v26.04) to macOS.
Fidelity to niri is the main goal: niri's own Rust code is reused wherever possible, and
only the Wayland/Smithay backend is replaced.

## Ground rules

These decisions are fixed. A change that goes against one of them won't be merged.

- **No private APIs.** System Integrity Protection stays enabled. ElaraWM uses the
  Accessibility API and other public frameworks only: no SkyLight or CGS calls, and no
  Dock injection.
- **niri's code is imported, not rewritten.** Its layout, config and IPC code is copied
  in and tracked in [UPSTREAM.md](UPSTREAM.md).
- **Workspaces are ElaraWM's own**, inside a single macOS Space per display. Native Spaces
  aren't used.
- **niri's config format.** The config is `~/.config/elara/config.kdl`. Settings ElaraWM
  adds go in the section they belong to and are marked as additions.
- **One binary**, `elara`, which runs either from a terminal or inside `ElaraWM.app`.
- **GPL-3.0-or-later**, the same as niri.

## Code layout

```
crates/
  elara-compat/   stand-ins for the Smithay types niri's code expects
  niri-ipc/       imported from niri
  niri-config/    imported from niri
  elara-layout/   niri's src/layout and src/animation, with inert rendering stand-ins
  elara-macos/    everything that talks to macOS: Accessibility, displays, input, drawing
  elara/          the daemon: state, actions, config loading, the main loop
```

The layout is authoritative. `elara-macos` feeds events in and applies the rectangles the
layout computes; it contains no layout logic. Keep macOS-facing code in `elara-macos`.

## Working with imported code

- Keep niri's file paths and names.
- Mark every change to an imported file with `// ELARA: <reason>`, so that a diff against
  a newer niri stays readable.
- Update the file table in [UPSTREAM.md](UPSTREAM.md) in the same commit.
- Don't remove drawing code from imported layout files. Rendering is neutralised by the
  stand-ins in `elara-layout/src/render_helpers` and `elara-compat`; see "How rendering is
  handled" in UPSTREAM.md. If a new niri render call fails to compile, fix the stand-in.
- If the pinned clippy flags unmodified niri code, allow the lint in that crate's
  `Cargo.toml` under `[lints.clippy]` with an `ELARA:` comment instead of editing the file.

## Git workflow

- Don't commit to `main` directly. Use one branch per unit of work: `feat/…`, `fix/…`,
  `chore/…`, `docs/…` or `test/…`.
- Write [conventional commit](https://www.conventionalcommits.org/) messages, with a body
  that explains why the change was made.
- Merge into `main` with `git merge --no-ff` once the checks below pass.

## Checks

Every change has to pass these before it's merged:

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo +nightly fmt --all -- --check
```

`rustfmt.toml` is niri's and uses unstable options, so formatting needs nightly rustfmt.

If you touched `elara-layout` or `elara-compat`, also run niri's slow exhaustive and
property-based layout tests. They take about 15 seconds in release mode:

```sh
RUN_SLOW_TESTS=1 cargo test --release -p elara-layout
```

## Testing

Everything above `elara-macos` is tested against `MockBackend`, which needs neither a
display nor any permission. Add a test there for any change in behavior.

Code in `elara-macos/src/macos/` can only be fully checked on a real desktop. Several
examples help with that, and most of them put nothing on screen:

```sh
cargo run -p elara-macos --example probe                         # permission, displays, windows
cargo run -p elara-macos --example overlay -- --raster out.rgba  # focus rings and borders, to a file
cargo run -p elara-macos --example overview -- --raster out.rgba # the overview, to a file
cargo run -p elara --example hotkey_overlay -- --raster out.rgba # the hotkey list, to a file
cargo run -p elara-macos --example thumbnails -- 4               # window previews, for 4 seconds
cargo run -p elara-macos --example ghosts -- 3                   # a preview sliding in from the top
```

The `--raster` examples write the image's width and height on the first line, followed by
raw RGBA bytes. `thumbnails` and `ghosts` need the Screen Recording permission and stop
without it. Each example's source describes its other options.

**Be careful running `elara` itself while developing.** Once it has the Accessibility
permission it takes over the desktop immediately: it moves and resizes every window and
intercepts the bound keys. If it's running and shouldn't be, stop it with
`pkill -INT -x elara`. SIGINT lets it restore your windows; SIGKILL leaves them parked off
screen.

Say so in your pull request if a change to `elara-macos/src/macos/` hasn't been run on a
real desktop.

## The default config

`resources/elara-default-config.kdl` is generated. Edit
`tools/gen-elara-default-config.py` and run it; don't edit the `.kdl` file by hand.
`resources/default-config.kdl` is niri's default and stays unmodified.

## Releases

1. Bump the version in the workspace `Cargo.toml` and in the path dependencies of
   `crates/elara/Cargo.toml`.
2. Add the release to [CHANGELOG.md](CHANGELOG.md). Write it for users: what they can now
   do, not which files changed.
3. Run `tools/release.sh`, which builds `ElaraWM.app` and writes
   `target/release/ElaraWM-<version>-<arch>.zip`. Don't zip the app by hand: the script
   strips build paths and extended attributes, and refuses to package a build that still
   contains the builder's home directory.
4. Tag the commit `v<version>` and attach the zip to a GitHub release.

Always sign releases with the same identity, or users have to grant the permissions again.

## Toolchain

Install [rustup](https://rustup.rs). The compiler version is pinned in
`rust-toolchain.toml`. If rustup comes from Homebrew, it's keg-only and `cargo` may not be
on your `PATH`:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```
