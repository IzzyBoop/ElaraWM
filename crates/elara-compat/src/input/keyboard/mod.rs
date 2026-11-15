//! Mirrors `smithay::input::keyboard`, which re-exports parts of the `xkbcommon` crate.
//!
//! niri identifies keys in binds by X11 keysym, parsed from names with libxkbcommon. That library
//! is not available on macOS, so the name lookup is reimplemented in [`xkb`] on top of a table
//! generated from libxkbcommon's own header.

#[rustfmt::skip]
mod keysym_names;
// ELARA: copied Smithay files are excluded from rustfmt and clippy so they stay verbatim.
#[rustfmt::skip]
#[allow(clippy::all)]
mod xkb_config;

pub use xkeysym::Keysym;

pub use self::xkb_config::XkbConfig;

/// Mirrors `xkbcommon::xkb::keysyms`. Only the constants niri's code uses are defined.
pub mod keysyms {
    #[allow(non_upper_case_globals)]
    pub const KEY_NoSymbol: u32 = 0x0000_0000;
}

/// Mirrors the keysym functions of `xkbcommon::xkb`.
pub mod xkb {
    use std::cmp::Ordering;

    use super::Keysym;
    use super::keysym_names::{KEYSYM_NAMES, KEYSYM_VALUES};
    use super::keysyms::KEY_NoSymbol;

    pub type KeysymFlags = u32;

    /// Do not apply any flags.
    pub const KEYSYM_NO_FLAGS: KeysymFlags = 0;
    /// Find keysym by case-insensitive search.
    pub const KEYSYM_CASE_INSENSITIVE: KeysymFlags = 1 << 0;

    /// Offset of the keysym range that encodes Unicode code points directly.
    const KEYSYM_UNICODE_OFFSET: u32 = 0x0100_0000;
    /// Largest valid keysym value.
    const KEYSYM_MAX: u32 = 0x1fff_ffff;

    /// Gets a keysym from its name.
    ///
    /// Follows `xkb_keysym_from_name()` from libxkbcommon:
    ///
    /// - Names are looked up in the keysym table. With [`KEYSYM_CASE_INSENSITIVE`], when several
    ///   names differ only by case, the one that sorts last wins, which is the lowercase one (`a`
    ///   over `A`, `XF86Screensaver` over `XF86ScreenSaver`).
    /// - `U` followed by hex digits is a Unicode code point.
    /// - `0x` followed by hex digits is a raw keysym value.
    /// - The legacy `XF86_` prefix is accepted for `XF86` names.
    ///
    /// Returns a keysym with the raw value `KEY_NoSymbol` if the name is not valid.
    pub fn keysym_from_name(name: &str, flags: KeysymFlags) -> Keysym {
        Keysym::new(keysym_from_name_raw(name, flags))
    }

    fn keysym_from_name_raw(name: &str, flags: KeysymFlags) -> u32 {
        if flags & !KEYSYM_CASE_INSENSITIVE != 0 {
            return KEY_NoSymbol;
        }
        let icase = flags & KEYSYM_CASE_INSENSITIVE != 0;

        if let Some(value) = lookup(name, icase) {
            return value;
        }

        let bytes = name.as_bytes();

        if bytes.first() == Some(&b'U') || (icase && bytes.first() == Some(&b'u')) {
            return match parse_keysym_hex(&name[1..]) {
                Some(val) if val < 0x20 || (val > 0x7e && val < 0xa0) => KEY_NoSymbol,
                Some(val) if val < 0x100 => val,
                Some(val) if val > 0x10_ffff => KEY_NoSymbol,
                Some(val) => val | KEYSYM_UNICODE_OFFSET,
                None => KEY_NoSymbol,
            };
        }

        if bytes.first() == Some(&b'0')
            && (bytes.get(1) == Some(&b'x') || (icase && bytes.get(1) == Some(&b'X')))
        {
            return match parse_keysym_hex(&name[2..]) {
                Some(val) if val <= KEYSYM_MAX => val,
                _ => KEY_NoSymbol,
            };
        }

        // Legacy spelling: "XF86_Foo" for "XF86Foo".
        if let Some(prefix) = name.get(..5) {
            if prefix == "XF86_" || (icase && prefix.eq_ignore_ascii_case("XF86_")) {
                let without_underscore = format!("{}{}", &name[..4], &name[5..]);
                return keysym_from_name_raw(&without_underscore, flags);
            }
        }

        KEY_NoSymbol
    }

    /// Gets the name of a keysym.
    ///
    /// Follows `xkb_keysym_get_name()` from libxkbcommon:
    ///
    /// - A keysym with names gets the first one the keysym header lists for it, which for `Page_Up`
    ///   is `Prior`.
    /// - A keysym that encodes a Unicode code point directly is `U` and the code point in hex.
    /// - Any other keysym is `0x` and its value in hex.
    pub fn keysym_get_name(keysym: Keysym) -> String {
        let raw = keysym.raw();
        if raw > KEYSYM_MAX {
            return String::from("Invalid");
        }

        if let Ok(idx) = KEYSYM_VALUES.binary_search_by_key(&raw, |(value, _)| *value) {
            return KEYSYM_VALUES[idx].1.to_owned();
        }

        if (0x0100_0100..=0x0110_ffff).contains(&raw) {
            let width = if raw & 0x00ff_0000 != 0 { 8 } else { 4 };
            return format!("U{:0width$X}", raw & 0x00ff_ffff);
        }

        format!("0x{raw:08x}")
    }

    /// Compares two names ignoring ASCII case, like libxkbcommon's `istrcmp()`.
    fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
        let a = a.bytes().map(|c| c.to_ascii_lowercase());
        let b = b.bytes().map(|c| c.to_ascii_lowercase());
        a.cmp(b)
    }

    fn lookup(name: &str, icase: bool) -> Option<u32> {
        // The table is sorted case-insensitively, so all names that differ only by case are
        // adjacent.
        let start = KEYSYM_NAMES.partition_point(|(n, _)| cmp_ignore_case(n, name).is_lt());
        let group = KEYSYM_NAMES[start..]
            .iter()
            .take_while(|(n, _)| n.eq_ignore_ascii_case(name));

        if icase {
            group.last().map(|(_, value)| *value)
        } else {
            let mut group = group;
            group.find(|(n, _)| *n == name).map(|(_, value)| *value)
        }
    }

    /// Parses 1 to 8 hex digits spanning the whole string.
    fn parse_keysym_hex(s: &str) -> Option<u32> {
        if s.is_empty() || s.len() > 8 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(s, 16).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn exact(name: &str) -> Keysym {
            keysym_from_name(name, KEYSYM_NO_FLAGS)
        }

        fn icase(name: &str) -> Keysym {
            keysym_from_name(name, KEYSYM_CASE_INSENSITIVE)
        }

        #[test]
        fn table_is_sorted_for_lookup() {
            for pair in KEYSYM_NAMES.windows(2) {
                let (a, b) = (pair[0].0, pair[1].0);
                let order = cmp_ignore_case(a, b).then_with(|| a.cmp(b));
                assert!(order.is_lt(), "{a} must sort before {b}");
            }
        }

        #[test]
        fn values_are_sorted_for_lookup() {
            for pair in KEYSYM_VALUES.windows(2) {
                assert!(pair[0].0 < pair[1].0, "{} must sort first", pair[0].1);
            }
        }

        #[test]
        fn names_of_keysyms() {
            assert_eq!(keysym_get_name(Keysym::a), "a");
            assert_eq!(keysym_get_name(Keysym::A), "A");
            assert_eq!(keysym_get_name(Keysym::_1), "1");
            assert_eq!(keysym_get_name(Keysym::slash), "slash");
            assert_eq!(keysym_get_name(Keysym::Return), "Return");
            assert_eq!(keysym_get_name(Keysym::XF86_AudioMute), "XF86AudioMute");
            // The first name listed wins over later aliases.
            assert_eq!(keysym_get_name(Keysym::Prior), "Prior");
            assert_eq!(keysym_get_name(Keysym::Next), "Next");
            // Every name leads back to the keysym it came from.
            for (value, name) in KEYSYM_VALUES {
                assert_eq!(exact(name).raw(), value);
            }
        }

        #[test]
        fn names_of_keysyms_without_one() {
            assert_eq!(keysym_get_name(Keysym::new(0x0100_0440)), "U0440");
            assert_eq!(keysym_get_name(Keysym::new(0x0101_f600)), "U0001F600");
            assert_eq!(keysym_get_name(Keysym::new(0x0012_3456)), "0x00123456");
            assert_eq!(keysym_get_name(Keysym::new(0x2000_0000)), "Invalid");
        }

        #[test]
        fn exact_names() {
            assert_eq!(exact("a"), Keysym::a);
            assert_eq!(exact("A"), Keysym::A);
            assert_eq!(exact("Return"), Keysym::Return);
            assert_eq!(exact("Tab"), Keysym::Tab);
            assert_eq!(exact("ISO_Left_Tab"), Keysym::ISO_Left_Tab);
            assert_eq!(exact("XF86AudioRaiseVolume"), Keysym::XF86_AudioRaiseVolume);
            assert_eq!(exact("1"), Keysym::_1);
        }

        #[test]
        fn exact_is_case_sensitive() {
            assert_eq!(exact("return").raw(), KEY_NoSymbol);
            assert_eq!(exact("TAB").raw(), KEY_NoSymbol);
        }

        #[test]
        fn deprecated_aliases_resolve() {
            assert_eq!(exact("Page_Up"), Keysym::Prior);
            assert_eq!(exact("Page_Down"), Keysym::Next);
            assert_eq!(icase("page_up"), Keysym::Prior);
        }

        #[test]
        fn case_insensitive_prefers_lowercase() {
            assert_eq!(icase("A"), Keysym::a);
            assert_eq!(icase("a"), Keysym::a);
            assert_eq!(icase("RETURN"), Keysym::Return);
            assert_eq!(icase("Aacute"), Keysym::aacute);
        }

        #[test]
        fn screensaver_spellings() {
            // niri's bind parser relies on this exact behavior.
            assert_eq!(icase("XF86ScreenSaver"), Keysym::XF86_Screensaver);
            assert_eq!(icase("XF86Screensaver"), Keysym::XF86_Screensaver);
            assert_eq!(exact("XF86ScreenSaver"), Keysym::XF86_ScreenSaver);
            assert_eq!(exact("XF86Screensaver"), Keysym::XF86_Screensaver);
        }

        #[test]
        fn unicode_names() {
            assert_eq!(exact("U41").raw(), 0x41);
            assert_eq!(exact("U20AC").raw(), 0x0100_20ac);
            assert_eq!(exact("u20AC").raw(), KEY_NoSymbol);
            assert_eq!(icase("u20ac").raw(), 0x0100_20ac);
            // Control characters and out-of-range code points are rejected.
            assert_eq!(exact("U1F").raw(), KEY_NoSymbol);
            assert_eq!(exact("U80").raw(), KEY_NoSymbol);
            assert_eq!(exact("U110000").raw(), KEY_NoSymbol);
            // A real name starting with U still wins.
            assert_eq!(exact("Up"), Keysym::Up);
        }

        #[test]
        fn hex_names() {
            assert_eq!(exact("0xff0d"), Keysym::Return);
            assert_eq!(exact("0Xff0d").raw(), KEY_NoSymbol);
            assert_eq!(icase("0Xff0d"), Keysym::Return);
            assert_eq!(exact("0x20000000").raw(), KEY_NoSymbol);
            assert_eq!(exact("0x").raw(), KEY_NoSymbol);
            // A real name starting with 0 still wins.
            assert_eq!(exact("0"), Keysym::_0);
        }

        #[test]
        fn legacy_xf86_underscore() {
            assert_eq!(exact("XF86_AudioMute"), Keysym::XF86_AudioMute);
            assert_eq!(icase("xf86_audiomute"), Keysym::XF86_AudioMute);
        }

        #[test]
        fn invalid_names() {
            assert_eq!(exact("").raw(), KEY_NoSymbol);
            assert_eq!(icase("").raw(), KEY_NoSymbol);
            assert_eq!(icase("NotAKey").raw(), KEY_NoSymbol);
            assert_eq!(exact("Ünicode").raw(), KEY_NoSymbol);
            assert_eq!(keysym_from_name("a", 1 << 1).raw(), KEY_NoSymbol);
        }
    }
}
