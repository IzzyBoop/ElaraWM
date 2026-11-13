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

None yet.

| ElaraWM path | Upstream path | Status |
|---|---|---|

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
