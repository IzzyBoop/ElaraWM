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

None yet.

| ElaraWM path | Upstream path | Status |
|---|---|---|
