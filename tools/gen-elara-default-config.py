#!/usr/bin/env python3
"""Derive ElaraWM's default config from niri's.

Usage:
    tools/gen-elara-default-config.py

Reads resources/default-config.kdl, which is niri's default config and is kept unmodified, and
writes resources/elara-default-config.kdl. Run it again after re-syncing with a newer niri.

The result differs from niri's default only where a line names a Linux program or an action
that ElaraWM does not support. Each edit below asserts that the line it expects is there, so a
changed upstream default fails loudly instead of producing a subtly wrong file.
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "resources" / "default-config.kdl"
TARGET = ROOT / "resources" / "elara-default-config.kdl"

HEADER = """\
// ElaraWM default config.
//
// ElaraWM reads niri's config format: https://niri-wm.github.io/niri/Configuration:-Introduction
// This file is niri's default config with a few lines changed for macOS. The changes are marked
// with "ElaraWM:".
//
// What is different on macOS:
//
// - "Mod" is the Option key unless you set `mod-key` in the `input` section. Command is left
//   to your applications. A bind on Mod+<letter> takes over Option+<letter>, which macOS
//   otherwise uses to type special characters; remove a bind to get the character back.
// - Scrolling with two fingers on the trackpad while holding Mod drags the strip of columns
//   sideways, or the workspaces up and down. This stands in for niri's three-finger swipe,
//   which macOS keeps for itself. It needs no bind. The mouse wheel uses the
//   `WheelScroll...` binds below.
// - Holding Mod and dragging a window with the left mouse button moves it, and with the right
//   button resizes it by the nearest edge or corner, as in niri. This needs no bind either.
//   It replaces what macOS does with an Option-click on a window, which is to hide the
//   application you were in.
// - Sections that configure things macOS controls itself are read and ignored: `output`
//   modes and positions, keyboard layouts, touchpad and mouse settings, the cursor, and
//   layer rules.
// - Windows are drawn by macOS. The focus ring, borders and the insert hint are drawn on top
//   of them, in the gaps, from the settings in the `layout` section; a gradient shows as its
//   first color. Window corners are taken to have a radius of 16 unless a window rule sets
//   `geometry-corner-radius`. Shadows and the tab indicator are not shown yet, though their
//   settings are accepted. Of the animations, the strip of columns sliding sideways
//   and workspaces sliding up and down are shown; windows opening, closing and resizing
//   take effect at once. `off` in the `animations` section turns the sliding off as well.

"""

# (line that must exist exactly once, replacement lines)
REPLACEMENTS = [
    (
        "// You can configure outputs by their name, which you can find",
        [
            '// ElaraWM: macOS controls displays. This section is ignored, even without its "/-".',
            "// You can configure outputs by their name, which you can find",
        ],
    ),
    (
        'spawn-at-startup "waybar"',
        [
            "// ElaraWM: niri starts waybar here. macOS has its own menu bar.",
            '// spawn-at-startup "waybar"',
        ],
    ),
    (
        '    Mod+T hotkey-overlay-title="Open a Terminal: alacritty" { spawn "alacritty"; }',
        [
            "    // ElaraWM: niri starts alacritty here.",
            '    Mod+T hotkey-overlay-title="Open a Terminal" { spawn "open" "-a" "Terminal"; }',
        ],
    ),
    (
        '    Mod+D hotkey-overlay-title="Run an Application: fuzzel" { spawn "fuzzel"; }',
        [
            "    // ElaraWM: niri starts the fuzzel launcher here. Use Spotlight (Command+Space).",
            '    // Mod+D hotkey-overlay-title="Run an Application: fuzzel" { spawn "fuzzel"; }',
        ],
    ),
    (
        '    Super+Alt+L hotkey-overlay-title="Lock the Screen: swaylock" { spawn "swaylock"; }',
        [
            "    // ElaraWM: niri starts swaylock here. macOS locks with Control+Command+Q.",
            '    // Super+Alt+L hotkey-overlay-title="Lock the Screen: swaylock" { spawn "swaylock"; }',
        ],
    ),
]

# Lines to comment out, each with the reason given once before the first of its group.
COMMENT_OUT = [
    (
        "ElaraWM: macOS sets Num Lock, tap-to-click and scroll direction in System Settings.",
        ["        numlock", "        tap", "        natural-scroll"],
    ),
    (
        "ElaraWM: the hotkey overlay is not supported yet.",
        ["    Mod+Shift+Slash { show-hotkey-overlay; }"],
    ),
    (
        "ElaraWM: the Orca screen reader is a Linux program. macOS has VoiceOver (Command+F5).",
        [
            '    Super+Alt+S allow-when-locked=true hotkey-overlay-title=null { spawn-sh "pkill orca || exec orca"; }',
        ],
    ),
    (
        "ElaraWM: macOS handles the volume, media and brightness keys itself.",
        [
            '    XF86AudioRaiseVolume allow-when-locked=true { spawn-sh "wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.1+ -l 1.0"; }',
            '    XF86AudioLowerVolume allow-when-locked=true { spawn-sh "wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.1-"; }',
            '    XF86AudioMute        allow-when-locked=true { spawn-sh "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle"; }',
            '    XF86AudioMicMute     allow-when-locked=true { spawn-sh "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle"; }',
            '    XF86AudioPlay        allow-when-locked=true { spawn-sh "playerctl play-pause"; }',
            '    XF86AudioStop        allow-when-locked=true { spawn-sh "playerctl stop"; }',
            '    XF86AudioPrev        allow-when-locked=true { spawn-sh "playerctl previous"; }',
            '    XF86AudioNext        allow-when-locked=true { spawn-sh "playerctl next"; }',
            '    XF86MonBrightnessUp allow-when-locked=true { spawn "brightnessctl" "--class=backlight" "set" "+10%"; }',
            '    XF86MonBrightnessDown allow-when-locked=true { spawn "brightnessctl" "--class=backlight" "set" "10%-"; }',
        ],
    ),
    (
        "ElaraWM: the overview is not supported yet.",
        ["    Mod+O repeat=false { toggle-overview; }"],
    ),
    (
        "ElaraWM: screenshots are not supported. macOS has Shift+Command+3, 4 and 5.",
        [
            "    Print { screenshot; }",
            "    Ctrl+Print { screenshot-screen; }",
            "    Alt+Print { screenshot-window; }",
        ],
    ),
    (
        "ElaraWM: there is no shortcut inhibiting on macOS.",
        ["    Mod+Escape allow-inhibiting=false { toggle-keyboard-shortcuts-inhibit; }"],
    ),
    (
        "ElaraWM: on a Mac, Control+Option+Delete is too easy to hit while editing text.",
        ["    Ctrl+Alt+Delete { quit; }"],
    ),
    (
        "ElaraWM: powering monitors off is not supported.",
        ["    Mod+Shift+P { power-off-monitors; }"],
    ),
]


def find_once(lines: list[str], wanted: str) -> int:
    hits = [i for i, line in enumerate(lines) if line == wanted]
    if len(hits) != 1:
        sys.exit(f"expected exactly one line {wanted!r} in {SOURCE.name}, found {len(hits)}")
    return hits[0]


def main() -> None:
    lines = SOURCE.read_text(encoding="utf-8").split("\n")

    for wanted, replacement in REPLACEMENTS:
        index = find_once(lines, wanted)
        lines[index : index + 1] = replacement

    for reason, group in COMMENT_OUT:
        for position, wanted in enumerate(group):
            index = find_once(lines, wanted)
            indent = wanted[: len(wanted) - len(wanted.lstrip())]
            commented = [f"{indent}// {wanted.lstrip()}"]
            if position == 0:
                commented.insert(0, f"{indent}// {reason}")
            lines[index : index + 1] = commented

    TARGET.write_text(HEADER + "\n".join(lines), encoding="utf-8")
    print(f"wrote {TARGET.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
