//! Checks against the real system. They pass trivially where the system cannot answer, such as
//! on a build machine.
#![cfg(target_os = "macos")]

#[test]
fn letter_keys_translate_through_the_keyboard_layout() {
    // Key code 0x03 is the key labelled F on an ANSI keyboard. What it types depends on the
    // layout in use, but on any layout it is a single printable character, and without
    // modifiers it is never the Option-layer symbol a US layout has there.
    let Some(character) = elara_macos::character_for_keycode(0x03) else {
        // An input method without a key layout of its own is active.
        return;
    };
    assert!(!character.is_control());
    assert_ne!(character, 'ƒ');

    // Keys that type nothing have no character.
    assert_eq!(elara_macos::character_for_keycode(0x7B), None, "left arrow");
}
