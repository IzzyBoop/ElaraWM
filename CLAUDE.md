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
- niri's animation clock stays in the core; window positions snap by default.
- Built as a cargo daemon for now. Keep macOS-facing code in `elara-macos` behind traits so a
  `.app` bundle can wrap it later.
- License is GPL-3.0-or-later.

## Layout

```
crates/
  elara-compat/   stand-ins for the Smithay types niri's code expects
  niri-ipc/       imported from upstream
  niri-config/    imported from upstream
  elara-layout/   niri src/layout and src/animation, rendering removed
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

## Toolchain

rustup is installed with Homebrew and is keg-only. If `cargo` is not on `PATH`:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```
