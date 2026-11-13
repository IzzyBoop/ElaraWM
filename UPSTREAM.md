# Upstream tracking

ElaraWM is derived from [niri](https://github.com/niri-wm/niri), a scrollable-tiling Wayland
compositor by Ivan Molodetskikh and contributors, licensed GPL-3.0-or-later.

| | |
|---|---|
| Repository | https://github.com/niri-wm/niri |
| Tag | `v26.04` |
| Commit | `8ed0da44d974c32c6877d2f4630c314da0717ecb` |
| Smithay rev used by that tag | `ff5fa7df392cecfba049ffed55cdaa4e98a8e7ef` |

## Conventions

- Imported files keep their upstream path and name inside the crate that hosts them.
- Every change to an imported file is marked with an `// ELARA:` comment explaining the
  divergence, so a diff against a newer niri tag stays readable.
- This file lists every imported file and its status:
  - **verbatim**: byte-identical to upstream
  - **modified**: upstream logic kept, with marked changes
  - **stubbed**: upstream API kept, body replaced (typically rendering)

## Re-syncing with a newer niri tag

1. Diff upstream between the tag recorded here and the new tag, limited to the imported paths.
2. Apply the upstream changes to the imported files, keeping `// ELARA:` divergences.
3. Run the full merge gate (see `CLAUDE.md`).
4. Update the tag and commit in the table above.

## Imported files

### From niri

Whole directories are listed once; every file in them is verbatim unless named separately.

| ElaraWM path | Upstream path | Status |
|---|---|---|
| `crates/niri-ipc/` | `niri-ipc/` | verbatim |
| `crates/niri-ipc/Cargo.toml` | `niri-ipc/Cargo.toml` | modified: explicit version |
| `crates/niri-config/` | `niri-config/` | verbatim |
| `crates/niri-config/Cargo.toml` | `niri-config/Cargo.toml` | modified: explicit version, Smithay stand-in, clippy allowances |
| `crates/niri-config/src/lib.rs` | `niri-config/src/lib.rs` | modified: path to the default config |
| `crates/niri-config/tests/wiki-parses.rs` | `niri-config/tests/wiki-parses.rs` | modified: path to the wiki |
| `crates/elara-layout/src/animation/` | `src/animation/` | verbatim |
| `crates/elara-layout/src/layout/` | `src/layout/` | verbatim, tests included |
| `crates/elara-layout/src/layout/workspace.rs` | `src/layout/workspace.rs` | modified: `configure_new_window()` removed |
| `crates/elara-layout/src/rubber_band.rs` | `src/rubber_band.rs` | verbatim |
| `crates/elara-layout/src/input/swipe_tracker.rs` | `src/input/swipe_tracker.rs` | verbatim |
| `crates/elara-layout/src/utils/id.rs` | `src/utils/id.rs` | verbatim |
| `crates/elara-layout/src/utils/transaction.rs` | `src/utils/transaction.rs` | verbatim |
| `crates/elara-layout/src/utils/mod.rs` | `src/utils/mod.rs` | modified: Wayland-specific helpers and unused submodules removed |
| `crates/elara-layout/src/window/mod.rs` | `src/window/mod.rs` | modified: `WindowRef` carries window facts instead of wrapping Wayland surfaces |
| `crates/elara-layout/src/render_helpers/` | `src/render_helpers/` | stubbed, see below |
| `resources/default-config.kdl` | `resources/default-config.kdl` | verbatim |
| `docs/wiki/*.md` | `docs/wiki/*.md` | verbatim (images and assets not imported) |

`crates/elara-layout/src/input/mod.rs` and `crates/elara-layout/src/lib.rs` are ElaraWM's own;
they only declare the imported modules.

#### How rendering is handled

niri's layout code computes where windows go and also draws them, through Smithay's OpenGL
renderer. On macOS the window server draws application windows. Instead of editing the drawing
code out of niri's files, which would make every later re-sync a manual merge, the files are kept
and the things they draw with are replaced:

- `crates/elara-layout/src/render_helpers/` has the same module names, type names and function
  signatures as niri's `src/render_helpers/`, written for ElaraWM. Elements that describe simple
  shapes (solid colors, borders, shadows) keep the parameters niri computed for them. Items that
  are plain data upstream (`RenderCtx`, `RenderTarget`, `BakedBuffer`, `XrayPos`,
  `SolidColorBuffer`, `ExtraDamage`, `RoundedCornerDamage`) are copied.
- `elara-compat` provides `GlesRenderer`, `GlesTexture` and a few related types as enums with no
  variants. Code that needs a renderer or a texture compiles, and the compiler proves it cannot
  run. The resulting unreachable-code lints are silenced for the `layout` module only, in
  `crates/elara-layout/src/lib.rs`.
- `smithay::desktop::layer_map_for_output()`, which niri uses to find the area left over by
  panels, is backed by a per-output value that the macOS backend sets to the display's visible
  frame.

When re-syncing, a new upstream call into `render_helpers` or Smithay's renderer shows up as a
compile error in an otherwise unmodified file; the fix belongs in the stand-in, not in the
imported file.

The wiki pages are niri's documentation, kept unedited because `niri-config`'s tests parse every
KDL snippet in them. They describe niri on Linux, not ElaraWM.

### From Smithay

[Smithay](https://github.com/Smithay/smithay) is MIT licensed; its license text is kept in
`crates/elara-compat/LICENSE-SMITHAY.txt`. Files are taken from the revision niri v26.04 pins
(see the table at the top). Paths are relative to `crates/elara-compat/src/` and Smithay's `src/`.

| ElaraWM path | Upstream path | Status |
|---|---|---|
| `utils/geometry.rs` | `utils/geometry.rs` | verbatim |
| `utils/serial.rs` | `utils/serial.rs` | verbatim |
| `utils/user_data.rs` | `utils/user_data.rs` | verbatim |
| `backend/renderer/color.rs` | `backend/renderer/color.rs` | verbatim |
| `output.rs` | `output.rs` | verbatim |
| `input/keyboard/xkb_config.rs` | `input/keyboard/xkb_config.rs` | modified: keymap compilation removed |

The rest of `elara-compat` (renderer, element, Wayland and desktop stand-ins, libinput enums,
keysym lookup) is written for ElaraWM and only mirrors Smithay's paths and names.

Verbatim Smithay files are excluded from rustfmt and clippy through attributes on their `mod`
declarations. Their Wayland and DRM parts sit behind Cargo features that `elara-compat` does not
define, so they compile out.

### From libxkbcommon

`crates/elara-compat/src/input/keyboard/keysym_names.rs` is generated from
`include/xkbcommon/xkbcommon-keysyms.h` of
[libxkbcommon](https://github.com/xkbcommon/libxkbcommon) at tag `xkbcommon-1.11.0` (MIT):

```sh
python3 crates/elara-compat/tools/gen_keysym_names.py path/to/xkbcommon-keysyms.h \
    > crates/elara-compat/src/input/keyboard/keysym_names.rs
```

The lookup in `input/keyboard/mod.rs` is a reimplementation of `xkb_keysym_from_name()`, not a
copy.
