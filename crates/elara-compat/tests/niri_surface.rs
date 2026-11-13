//! Checks that every Smithay path niri's config and layout code imports exists in the shim and
//! behaves as that code expects.

use smithay::backend::renderer::Color32F;
use smithay::input::keyboard::keysyms::KEY_NoSymbol;
use smithay::input::keyboard::xkb::{KEYSYM_CASE_INSENSITIVE, KEYSYM_NO_FLAGS, keysym_from_name};
use smithay::input::keyboard::{Keysym, XkbConfig};
use smithay::output::{self, Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::input;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Serial, Size, Transform};

fn test_output(name: &str) -> Output {
    Output::new(
        name.to_owned(),
        PhysicalProperties {
            size: Size::from((600, 340)),
            subpixel: Subpixel::Unknown,
            make: "Elara".to_owned(),
            model: "Test".to_owned(),
            serial_number: "0".to_owned(),
        },
    )
}

#[test]
fn output_state_round_trips() {
    let output = test_output("display-1");
    assert_eq!(output.name(), "display-1");
    assert_eq!(output.current_mode(), None);

    let mode = Mode {
        size: Size::<i32, Physical>::from((3024, 1964)),
        refresh: 120_000,
    };
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(output::Scale::Fractional(2.0)),
        Some(Point::<i32, Logical>::from((0, 0))),
    );

    assert_eq!(output.current_mode(), Some(mode));
    assert_eq!(output.current_transform(), Transform::Normal);
    assert_eq!(output.current_scale().fractional_scale(), 2.0);
    assert_eq!(output.current_scale().integer_scale(), 2);
}

#[test]
fn output_identity_and_user_data() {
    let a = test_output("a");
    let b = test_output("b");
    assert_eq!(a, a.clone());
    assert_ne!(a, b);
    assert_eq!(a.downgrade().upgrade(), Some(a.clone()));

    assert!(a.user_data().insert_if_missing(|| 7u32));
    assert_eq!(a.clone().user_data().get::<u32>(), Some(&7));
    assert_eq!(b.user_data().get::<u32>(), None);
}

#[test]
fn logical_geometry() {
    let rect = Rectangle::<f64, Logical>::new(Point::from((10., 20.)), Size::from((300., 200.)));
    assert!(rect.contains(Point::from((10., 20.))));
    assert!(!rect.contains(Point::from((310., 20.))));

    let physical = rect.to_physical_precise_round(Scale::from(2.0));
    let expected: Rectangle<i32, Physical> =
        Rectangle::new(Point::from((20, 40)), Size::from((600, 400)));
    assert_eq!(physical, expected);
}

#[test]
fn serials_compare_with_wraparound() {
    let old = Serial::from(u32::MAX - 1);
    let new = Serial::from(1);
    assert!(new > old);
}

#[test]
fn keysym_lookup_as_used_by_binds() {
    assert_eq!(keysym_from_name("q", KEYSYM_CASE_INSENSITIVE), Keysym::q);
    assert_eq!(
        keysym_from_name("nope", KEYSYM_NO_FLAGS).raw(),
        KEY_NoSymbol
    );
}

#[test]
fn config_only_types_exist() {
    let _ = Color32F::from([0.5, 0.5, 0.5, 1.0]);
    let _ = XkbConfig::default();
    let _ = input::ClickMethod::Clickfinger;
    let _ = input::AccelProfile::Flat;
    let _ = input::ScrollMethod::TwoFinger;
    let _ = input::TapButtonMap::LeftRightMiddle;
}
