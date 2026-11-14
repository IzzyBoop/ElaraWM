//! ElaraWM's item in the menu bar: its mark, the current workspace, and a short menu.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AllocAnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSCellImagePosition, NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu,
    NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_core_foundation::CGSize;
use objc2_foundation::{ns_string, NSData, NSObject, NSObjectProtocol, NSString};

/// Something the user chose from the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    /// Open the config file in the default editor.
    OpenConfig,
    /// Show the log file.
    ShowLog,
    /// Switch starting at login on or off.
    ToggleStartAtLogin,
    Quit,
}

impl MenuCommand {
    const ALL: [Self; 4] = [
        Self::OpenConfig,
        Self::ShowLog,
        Self::ToggleStartAtLogin,
        Self::Quit,
    ];

    fn tag(self) -> isize {
        Self::ALL.iter().position(|c| *c == self).unwrap() as isize
    }

    fn from_tag(tag: isize) -> Option<Self> {
        Self::ALL.get(usize::try_from(tag).ok()?).copied()
    }
}

thread_local! {
    /// Filled when a menu item is chosen, which happens on the main thread, from inside the
    /// event loop.
    static CHOSEN: RefCell<Vec<MenuCommand>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and the class does not implement
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ElaraMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        /// The action of every item in the menu.
        #[unsafe(method(itemChosen:))]
        fn item_chosen(&self, sender: &NSMenuItem) {
            if let Some(command) = MenuCommand::from_tag(sender.tag()) {
                CHOSEN.with(|chosen| chosen.borrow_mut().push(command));
            }
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `init` is the designated initializer of `NSObject`.
        unsafe { msg_send![super(this), init] }
    }
}

/// The item in the menu bar. It is there for as long as this exists.
pub struct MenuBar {
    mtm: MainThreadMarker,
    item: Retained<NSStatusItem>,
    /// The first line of the menu, which names the workspace.
    header: Retained<NSMenuItem>,
    log: Retained<NSMenuItem>,
    login: Retained<NSMenuItem>,
    /// Menu items do not keep their target alive.
    _target: Retained<MenuTarget>,
    workspace: String,
    start_at_login: Option<bool>,
}

impl MenuBar {
    /// Puts the item in the menu bar. `mark` is the mark as the contents of an SVG file: a
    /// black shape on a transparent background, which the system recolors to suit the bar.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the main thread.
    pub fn new(mark: &[u8]) -> Self {
        let mtm = MainThreadMarker::new().expect("the menu bar item is made on the main thread");
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);

        if let Some(button) = item.button(mtm) {
            let image = NSImage::initWithData(
                <NSImage as AllocAnyThread>::alloc(),
                &NSData::with_bytes(mark),
            );
            if let Some(image) = image {
                // As tall as menu bar icons are, and as wide as the mark's shape makes it.
                let size = image.size();
                let height = 14.;
                let width = if size.height > 0. {
                    (size.width * height / size.height).round()
                } else {
                    height
                };
                image.setSize(CGSize::new(width, height));
                image.setTemplate(true);
                button.setImage(Some(&image));
                button.setImagePosition(NSCellImagePosition::ImageLeading);
            } else {
                warn!("the menu bar mark could not be read; showing a name instead");
                button.setTitle(ns_string!("Elara"));
            }
        }

        let target = MenuTarget::new(mtm);
        let menu = NSMenu::new(mtm);
        // Items are enabled and disabled here, not by AppKit asking the target.
        menu.setAutoenablesItems(false);

        let add = |title: &NSString, command: Option<MenuCommand>| {
            // SAFETY: the action takes the one argument AppKit passes, the item that was
            // chosen, and the target outlives the menu: both are owned by `MenuBar`.
            let item = unsafe {
                let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    title,
                    command.map(|_| sel!(itemChosen:)),
                    ns_string!(""),
                );
                let target: &AnyObject = &target;
                item.setTarget(command.map(|_| target));
                item
            };
            match command {
                Some(command) => item.setTag(command.tag()),
                None => item.setEnabled(false),
            }
            menu.addItem(&item);
            item
        };

        let header = add(ns_string!("ElaraWM"), None);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add(ns_string!("Open Config"), Some(MenuCommand::OpenConfig));
        let log = add(ns_string!("Show Log"), Some(MenuCommand::ShowLog));
        let login = add(
            ns_string!("Start at Login"),
            Some(MenuCommand::ToggleStartAtLogin),
        );
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add(ns_string!("Quit ElaraWM"), Some(MenuCommand::Quit));

        item.setMenu(Some(&menu));

        let mut bar = Self {
            mtm,
            item,
            header,
            log,
            login,
            _target: target,
            workspace: String::new(),
            start_at_login: Some(false),
        };
        bar.set_start_at_login(None);
        bar
    }

    /// Shows which workspace is active: `label` next to the mark, and spelled out in the
    /// menu.
    pub fn set_workspace(&mut self, label: &str) {
        if self.workspace == label {
            return;
        }
        self.workspace = label.to_owned();
        if let Some(button) = self.item.button(self.mtm) {
            // A little room between the mark and the label.
            button.setTitle(&NSString::from_str(&format!(" {label}")));
        }
        self.header
            .setTitle(&NSString::from_str(&format!("Workspace {label}")));
    }

    /// Shows `text` as the first line of the menu and nothing next to the mark, for when
    /// there is something to say other than which workspace is active.
    pub fn set_status(&mut self, text: &str) {
        self.workspace.clear();
        if let Some(button) = self.item.button(self.mtm) {
            button.setTitle(ns_string!(""));
        }
        self.header.setTitle(&NSString::from_str(text));
    }

    /// Greys out the item that shows the log, for when the log goes to the terminal.
    pub fn set_log_available(&mut self, available: bool) {
        self.log.setEnabled(available);
    }

    /// Shows whether ElaraWM starts at login, as a checkmark. `None` greys the item out,
    /// for when that cannot be set up, as when not running from an application bundle.
    pub fn set_start_at_login(&mut self, enabled: Option<bool>) {
        if self.start_at_login == enabled {
            return;
        }
        self.start_at_login = enabled;
        self.login.setEnabled(enabled.is_some());
        self.login.setState(if enabled == Some(true) {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }

    /// Returns what the user chose from the menu since the last call, oldest first.
    pub fn take_commands(&mut self) -> Vec<MenuCommand> {
        CHOSEN.with(|chosen| std::mem::take(&mut *chosen.borrow_mut()))
    }

    /// Whether the system has given the item a place in the menu bar. For checking that it
    /// shows.
    pub fn is_shown(&self) -> bool {
        self.item
            .button(self.mtm)
            .and_then(|button| button.window())
            .is_some_and(|window| window.isVisible())
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}
