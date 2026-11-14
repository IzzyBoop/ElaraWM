//! What ElaraWM needs as an application: whether it runs from a bundle, starting at login,
//! and telling the user something when there is no terminal to print to.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{NSAlert, NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::{NSBundle, NSError, NSString};

// `SMAppService` lives here. There are no bindings for the framework among the crates in
// use, and the three messages below do not justify adding some.
#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// `SMAppServiceStatusEnabled`.
const STATUS_ENABLED: isize = 1;
/// `SMAppServiceStatusRequiresApproval`: registered, but switched off by the user in System
/// Settings, under General, Login Items.
const STATUS_REQUIRES_APPROVAL: isize = 2;

/// Returns the identifier of the application bundle this process runs from, or `None` if it
/// is a plain executable, as with `cargo run`.
pub fn bundle_identifier() -> Option<String> {
    NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|identifier| identifier.to_string())
}

/// The service that stands for this application as a login item.
fn login_service() -> Option<Retained<AnyObject>> {
    // Starting at login is registered for a bundle, by its identifier.
    bundle_identifier()?;
    let class = AnyClass::get(c"SMAppService")?;
    // SAFETY: `+[SMAppService mainAppService]` takes no arguments and returns an object.
    unsafe { msg_send![class, mainAppService] }
}

/// Returns whether ElaraWM is set to start at login. `None` if that cannot be set up, which
/// is the case when not running from an application bundle.
pub fn start_at_login() -> Option<bool> {
    let service = login_service()?;
    // SAFETY: `-[SMAppService status]` returns an `NSInteger` enumeration.
    let status: isize = unsafe { msg_send![&*service, status] };
    Some(status == STATUS_ENABLED)
}

/// Switches starting at login on or off.
///
/// On success, returns whether it is on now. It can still be off after being switched on: the
/// user may have turned ElaraWM off in System Settings, under General, Login Items, and only
/// they can turn it back on there.
pub fn set_start_at_login(enabled: bool) -> Result<bool, String> {
    let service = login_service().ok_or("not running from an application bundle")?;
    // SAFETY: both methods take a pointer to where to put an error and return whether they
    // succeeded, which is the convention `_` stands for.
    let result: Result<(), Retained<NSError>> = unsafe {
        if enabled {
            msg_send![&*service, registerAndReturnError: _]
        } else {
            msg_send![&*service, unregisterAndReturnError: _]
        }
    };
    // SAFETY: as in `start_at_login()`.
    let status: isize = unsafe { msg_send![&*service, status] };
    match result {
        Ok(()) => Ok(status == STATUS_ENABLED),
        // Registered, and waiting for the user: not a failure of ours.
        Err(_) if enabled && status == STATUS_REQUIRES_APPROVAL => Ok(false),
        Err(error) => Err(error.localizedDescription().to_string()),
    }
}

/// Shows a dialog with one button and waits for the user to dismiss it. For what would go to
/// the terminal if there were one.
///
/// # Panics
///
/// Panics if called from a thread other than the main thread.
pub fn alert(title: &str, text: &str) {
    let mtm = MainThreadMarker::new().expect("dialogs are shown on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    // An accessory is not brought forward by being launched, and the dialog would open behind
    // whatever is in front.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(text));
    alert.runModal();
}
