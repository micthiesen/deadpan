//! Main-thread adapter for winit 0.30.13's WinitApplicationDelegate.
//!
//! AppKit asks `applicationShouldTerminate:` before its existing
//! `applicationWillTerminate:`. winit supplies only the latter. Add the former
//! only after checking the exact class and absence of a competing method.
//! Unsafe is confined to ABI registration and an owned, alert-only keyboard
//! event monitor. The monitor is removed before the native alert is released.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, ProtocolObject, Sel};
use objc2::{Encode, MainThreadMarker, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertStyle, NSApplication,
    NSApplicationDelegate, NSApplicationTerminateReply, NSButton, NSButtonCell, NSEvent,
    NSEventMask, NSEventModifierFlags, NSWindow,
};
use objc2_foundation::NSString;

use crate::{Decision, PreviewState};

thread_local! {
    static ACTIVE: RefCell<Option<Weak<State>>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

struct State {
    delegate: Retained<ProtocolObject<dyn NSApplicationDelegate>>,
    previews: RefCell<PreviewState>,
}

/// Main-thread, single-owner lifetime for native termination confirmation.
/// Keep it alive until after the host's shutdown callback has run.
pub struct QuitGuard {
    state: Rc<State>,
}

impl QuitGuard {
    /// Install after eframe has created its event loop and application delegate.
    /// An unsupported host fails explicitly rather than silently losing drafts.
    pub fn install() -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("Quit guard requires the main thread")?;
        if ACTIVE.with(|active| active.borrow().as_ref().and_then(Weak::upgrade).is_some()) {
            return Err("A native quit guard is already installed".into());
        }
        let app = NSApplication::sharedApplication(mtm);
        let delegate = app
            .delegate()
            .ok_or("The window host has no application delegate")?;
        let object: &AnyObject = (*delegate).as_ref();
        let class = object.class();
        if class.name() != c"WinitApplicationDelegate"
            || class
                .instance_method(sel!(applicationWillTerminate:))
                .is_none()
        {
            return Err(format!(
                "Unsupported application delegate for quit confirmation: {:?}",
                class.name()
            ));
        }
        install_method(class)?;
        let state = Rc::new(State {
            delegate,
            previews: RefCell::new(PreviewState::default()),
        });
        ACTIVE.with(|active| *active.borrow_mut() = Some(Rc::downgrade(&state)));
        // Refresh AppKit's optional-delegate-method cache without replacing
        // winit's delegate or any of the references its windows retain.
        app.setDelegate(Some(&state.delegate));
        Ok(Self { state })
    }

    /// Publish the complete list from the final editor state of each frame.
    /// No app borrows remain live while AppKit runs its native confirmation.
    pub fn update(&self, previews: &[&'static str]) {
        self.state.previews.borrow_mut().update(previews);
    }
}

#[allow(unsafe_code)]
fn install_method(class: &AnyClass) -> Result<(), String> {
    let selector = sel!(applicationShouldTerminate:);
    // SAFETY: Objective-C's untyped IMP has this exact ABI after registration:
    // self and selector followed by NSApplication*, returning NSUInteger.
    // All are non-null framework-owned objects valid throughout the callback.
    let implementation: Imp = unsafe {
        std::mem::transmute::<
            extern "C-unwind" fn(&AnyObject, Sel, &NSApplication) -> NSApplicationTerminateReply,
            Imp,
        >(should_terminate)
    };
    if let Some(existing) = class.instance_method(selector) {
        return if INSTALLED.get() && std::ptr::fn_addr_eq(existing.implementation(), implementation)
        {
            Ok(())
        } else {
            Err(
                "The window host already implements quit confirmation; its handler was preserved"
                    .into(),
            )
        };
    }
    let encoding = std::ffi::CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))
        .map_err(|error| error.to_string())?;
    // SAFETY: class is the checked, live winit class. No method is overwritten;
    // class_addMethod atomically refuses if one already exists. The function
    // lives for the process lifetime and matches the encoded selector ABI.
    let added = unsafe {
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            selector,
            implementation,
            encoding.as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err("Could not install native quit confirmation".into());
    }
    INSTALLED.set(true);
    Ok(())
}

extern "C-unwind" fn should_terminate(
    delegate: &AnyObject,
    _selector: Sel,
    _app: &NSApplication,
) -> NSApplicationTerminateReply {
    // Neither a Rust panic nor missing adapter state can authorize losing drafts.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| confirm_quit(delegate)))
        .unwrap_or(NSApplicationTerminateReply::TerminateCancel)
}

fn confirm_quit(delegate: &AnyObject) -> NSApplicationTerminateReply {
    let Some(mtm) = MainThreadMarker::new() else {
        return NSApplicationTerminateReply::TerminateCancel;
    };
    let Some(state) = ACTIVE.with(|active| active.borrow().as_ref().and_then(Weak::upgrade)) else {
        return NSApplicationTerminateReply::TerminateCancel;
    };
    if !std::ptr::eq::<AnyObject>((*state.delegate).as_ref(), delegate) {
        return NSApplicationTerminateReply::TerminateCancel;
    }
    let decision = state.previews.borrow_mut().begin();
    let Decision::Ask {
        generation,
        message,
    } = decision
    else {
        return match decision {
            Decision::Quit => NSApplicationTerminateReply::TerminateNow,
            _ => NSApplicationTerminateReply::TerminateCancel,
        };
    };
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(&NSString::from_str("Quit with unsaved previews?"));
    alert.setInformativeText(&NSString::from_str(&message));
    // Keep editing owns Return. A button has only one key equivalent, so
    // Escape is handled by a monitor scoped to this alert's modal lifetime.
    let keep = alert.addButtonWithTitle(&NSString::from_str("Keep editing"));
    let discard = alert.addButtonWithTitle(&NSString::from_str("Discard previews and quit"));
    discard.setKeyEquivalent(&NSString::from_str(""));
    let default_cell = keep
        .cell()
        .and_then(|cell| cell.downcast::<NSButtonCell>().ok());
    alert.window().setDefaultButtonCell(default_cell.as_deref());
    keep.setRefusesFirstResponder(false);
    discard.setRefusesFirstResponder(false);
    alert.window().setInitialFirstResponder(Some(&keep));
    alert.window().makeFirstResponder(Some(&keep));
    discard.setHasDestructiveAction(true);
    let Some(_keys) = AlertKeys::new(alert.window(), [keep, discard], mtm) else {
        state.previews.borrow_mut().finish(generation, false);
        return NSApplicationTerminateReply::TerminateCancel;
    };
    // A native modal runs in AppKit's modal mode. Do not return TerminateLater
    // and expect egui input: winit defers its handlers in that run-loop mode.
    let discard = alert.runModal() == NSAlertSecondButtonReturn;
    if state.previews.borrow_mut().finish(generation, discard) {
        NSApplicationTerminateReply::TerminateNow
    } else {
        NSApplicationTerminateReply::TerminateCancel
    }
}

/// A local monitor cannot see other applications' input. This one consumes
/// only this exact alert's Escape, button traversal and focused activation.
/// All other events retain the framework's original pointer and routing.
struct AlertKeys {
    token: Retained<AnyObject>,
    _main_thread: MainThreadMarker,
}

impl AlertKeys {
    #[allow(unsafe_code)]
    fn new(
        window: Retained<NSWindow>,
        buttons: [Retained<NSButton>; 2],
        mtm: MainThreadMarker,
    ) -> Option<Self> {
        let app = NSApplication::sharedApplication(mtm);
        let handler = block2::RcBlock::new(move |pointer: std::ptr::NonNull<NSEvent>| {
            // SAFETY: AppKit provides a live event for this synchronous local
            // monitor call. No reference or pointer is retained after return.
            let event = unsafe { pointer.as_ref() };
            let modifiers = NSEventModifierFlags::Command
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option;
            if event.modifierFlags().intersection(modifiers).is_empty()
                && event
                    .window(mtm)
                    .is_some_and(|target| std::ptr::eq(&*target, &*window))
            {
                let key = event
                    .charactersIgnoringModifiers()
                    .map(|text| text.to_string());
                let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
                let focused = window.firstResponder().and_then(|responder| {
                    buttons.iter().position(|button| {
                        std::ptr::eq(
                            std::ptr::from_ref(&*responder).cast::<AnyObject>(),
                            std::ptr::from_ref(&**button).cast::<AnyObject>(),
                        )
                    })
                });
                match key.as_deref() {
                    Some("\u{1b}") if !shift => {
                        app.stopModalWithCode(NSAlertFirstButtonReturn);
                        return std::ptr::null_mut();
                    }
                    Some("\t" | "\u{19}") => {
                        // Explicit focus traversal also works when macOS's
                        // optional full keyboard navigation setting is off.
                        let next = focused.map_or(usize::from(shift), |index| 1 - index);
                        if window.makeFirstResponder(Some(&buttons[next])) {
                            return std::ptr::null_mut();
                        }
                    }
                    Some("\r" | "\u{3}" | " ") if !shift => {
                        if let Some(index) = focused {
                            app.stopModalWithCode(if index == 0 {
                                NSAlertFirstButtonReturn
                            } else {
                                NSAlertSecondButtonReturn
                            });
                            return std::ptr::null_mut();
                        }
                    }
                    _ => {}
                }
            }
            pointer.as_ptr()
        });
        // SAFETY: The copied block returns either this invocation's original
        // live event pointer or null (consumed), exactly as AppKit requires.
        // It owns its window/application references and runs on the main thread.
        let token = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler)
        }?;
        Some(Self {
            token,
            _main_thread: mtm,
        })
    }
}

impl Drop for AlertKeys {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: This is the token returned by our one successful monitor
        // registration. Removal happens once, on its creating main thread.
        unsafe { NSEvent::removeMonitor(&self.token) };
    }
}
