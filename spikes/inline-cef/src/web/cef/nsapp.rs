//! Makes the process's `NSApplication` acceptable to CEF without taking it
//! away from GPUI.
//!
//! CEF's `cef_initialize` CHECKs that `NSApp` conforms to `CefAppProtocol`
//! (which extends Chromium's `CrAppControlProtocol`: an `isHandlingSendEvent`
//! flag raised around `-sendEvent:`). There is one `NSApp` per process, and its
//! class is whichever class first calls `+sharedApplication`. GPUI needs that
//! to be its own `GPUIApplication` (it stores its platform pointer in an ivar
//! of that class), so CEF can't be allowed to create a plain `NSApplication`.
//!
//! GPUI registers `GPUIApplication` from a `#[ctor]`, i.e. before `main`. So at
//! boot we can retrofit the protocol onto that class through the Objective-C
//! runtime and instantiate `NSApp` as it, *before* `cef_initialize`. GPUI's own
//! later `[GPUIApplication sharedApplication]` then returns the same instance.
//! (bokuweb/gpui-cef does the same retrofit but defers `cef_initialize` until
//! inside GPUI's run closure; doing it at boot keeps the engine fully started
//! when `boot()` returns.)
//!
//! When GPUI's class isn't linked (the `cef_smoke` example), a minimal
//! `NSApplication` subclass is registered instead.
//!
//! Raw runtime FFI rather than `objc2`, to keep the dependency list to `cef`.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

type Id = *mut c_void;
type Class = *mut c_void;
type Sel = *const c_void;
type Method = *mut c_void;
type Protocol = *mut c_void;
type Imp = unsafe extern "C" fn();
/// Objective-C `BOOL` (a C `bool` on arm64, `signed char` on x86_64; both one byte).
type Bool = i8;

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Class;
    fn objc_allocateClassPair(superclass: Class, name: *const c_char, extra: usize) -> Class;
    fn objc_registerClassPair(class: Class);
    fn object_getClass(object: Id) -> Class;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn class_addMethod(class: Class, name: Sel, imp: Imp, types: *const c_char) -> Bool;
    fn class_getInstanceMethod(class: Class, name: Sel) -> Method;
    fn class_getSuperclass(class: Class) -> Class;
    fn method_getImplementation(method: Method) -> Option<Imp>;
    fn method_setImplementation(method: Method, imp: Imp) -> Option<Imp>;
    fn objc_getProtocol(name: *const c_char) -> Protocol;
    fn objc_allocateProtocol(name: *const c_char) -> Protocol;
    fn objc_registerProtocol(protocol: Protocol);
    fn protocol_addMethodDescription(
        protocol: Protocol,
        name: Sel,
        types: *const c_char,
        is_required: Bool,
        is_instance: Bool,
    );
    fn class_addProtocol(class: Class, protocol: Protocol) -> Bool;
    fn class_conformsToProtocol(class: Class, protocol: Protocol) -> Bool;
    fn objc_msgSend();
}

const GPUI_APPLICATION: &CStr = c"GPUIApplication";
const FALLBACK_APPLICATION: &CStr = c"InlineCefApplication";

#[cfg(target_arch = "aarch64")]
const BOOL_GETTER_TYPES: &CStr = c"B@:";
#[cfg(not(target_arch = "aarch64"))]
const BOOL_GETTER_TYPES: &CStr = c"c@:";
#[cfg(target_arch = "aarch64")]
const BOOL_SETTER_TYPES: &CStr = c"v@:B";
#[cfg(not(target_arch = "aarch64"))]
const BOOL_SETTER_TYPES: &CStr = c"v@:c";

thread_local! {
    /// `NSApp` only lives on the main thread, so the flag can be thread-local.
    static HANDLING_SEND_EVENT: Cell<bool> = const { Cell::new(false) };
}

/// The `-sendEvent:` we wrap (NSApplication's, or the class's own override).
static ORIGINAL_SEND_EVENT: OnceLock<usize> = OnceLock::new();

extern "C" fn is_handling_send_event(_this: Id, _sel: Sel) -> Bool {
    HANDLING_SEND_EVENT.with(|flag| flag.get()) as Bool
}

extern "C" fn set_handling_send_event(_this: Id, _sel: Sel, handling: Bool) {
    HANDLING_SEND_EVENT.with(|flag| flag.set(handling != 0));
}

extern "C" fn send_event(this: Id, sel: Sel, event: Id) {
    let previous = HANDLING_SEND_EVENT.with(|flag| flag.replace(true));
    if let Some(&original) = ORIGINAL_SEND_EVENT.get() {
        // SAFETY: `original` is the IMP of `-sendEvent:`, whose signature this is.
        let original: extern "C" fn(Id, Sel, Id) = unsafe { std::mem::transmute(original) };
        original(this, sel, event);
    }
    HANDLING_SEND_EVENT.with(|flag| flag.set(previous));
}

fn sel(name: &CStr) -> Sel {
    unsafe { sel_registerName(name.as_ptr()) }
}

/// Make `NSApp` an instance of a class that conforms to `CefAppProtocol`.
/// Call on the main thread, after the CEF framework is loaded and before
/// `cef_initialize`, and before anything else touches `NSApp`.
pub(super) fn adopt_cef_app_protocol() -> Result<(), String> {
    unsafe {
        let class = application_class()?;

        let mut cef_protocol = objc_getProtocol(c"CefAppProtocol".as_ptr());
        let already = !cef_protocol.is_null() && class_conformsToProtocol(class, cef_protocol) != 0;
        if !already {
            retrofit(class)?;
            cef_protocol = objc_getProtocol(c"CefAppProtocol".as_ptr());
        }

        // Instantiate NSApp as this class now, before CEF can create a plain one.
        let shared_application: extern "C" fn(Class, Sel) -> Id =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let app = shared_application(class, sel(c"sharedApplication"));
        if app.is_null() {
            return Err("+sharedApplication returned nil".into());
        }
        let app_class = object_getClass(app);
        if class_conformsToProtocol(app_class, cef_protocol) == 0 {
            return Err(
                "NSApp already existed as a class without CefAppProtocol; \
                 call web::cef::boot() before anything touches NSApp"
                    .into(),
            );
        }
        Ok(())
    }
}

/// GPUI's `NSApplication` subclass if it is linked in, else a fresh subclass.
unsafe fn application_class() -> Result<Class, String> {
    let gpui = objc_getClass(GPUI_APPLICATION.as_ptr());
    if !gpui.is_null() {
        return Ok(gpui);
    }
    let existing = objc_getClass(FALLBACK_APPLICATION.as_ptr());
    if !existing.is_null() {
        return Ok(existing);
    }
    let ns_application = objc_getClass(c"NSApplication".as_ptr());
    if ns_application.is_null() {
        return Err("AppKit is not loaded (no NSApplication class)".into());
    }
    let class = objc_allocateClassPair(ns_application, FALLBACK_APPLICATION.as_ptr(), 0);
    if class.is_null() {
        return Err("could not allocate an NSApplication subclass".into());
    }
    objc_registerClassPair(class);
    Ok(class)
}

unsafe fn retrofit(class: Class) -> Result<(), String> {
    let as_imp = |f: *const c_void| -> Imp { std::mem::transmute(f) };

    if class_addMethod(
        class,
        sel(c"isHandlingSendEvent"),
        as_imp(is_handling_send_event as *const c_void),
        BOOL_GETTER_TYPES.as_ptr(),
    ) == 0
    {
        return Err("could not add -isHandlingSendEvent".into());
    }
    if class_addMethod(
        class,
        sel(c"setHandlingSendEvent:"),
        as_imp(set_handling_send_event as *const c_void),
        BOOL_SETTER_TYPES.as_ptr(),
    ) == 0
    {
        return Err("could not add -setHandlingSendEvent:".into());
    }

    // Wrap -sendEvent: on this class only. If the class doesn't override it,
    // add an override that calls the superclass's; if it does, swap its IMP.
    let send_event_sel = sel(c"sendEvent:");
    let superclass = class_getSuperclass(class);
    let inherited = class_getInstanceMethod(superclass, send_event_sel);
    let inherited_imp = method_getImplementation(inherited).ok_or("no -sendEvent: IMP")?;
    let wrapper = as_imp(send_event as *const c_void);
    if class_addMethod(class, send_event_sel, wrapper, c"v@:@".as_ptr()) != 0 {
        let _ = ORIGINAL_SEND_EVENT.set(inherited_imp as usize);
    } else {
        let own = class_getInstanceMethod(class, send_event_sel);
        let own_imp = method_getImplementation(own).ok_or("no -sendEvent: IMP")?;
        let _ = ORIGINAL_SEND_EVENT.set(own_imp as usize);
        method_setImplementation(own, wrapper);
    }

    // Chromium and CEF test conformsToProtocol:, which matches by name. The
    // framework may or may not have registered these protocols; register any
    // that are missing.
    for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
        let mut protocol = objc_getProtocol(name.as_ptr());
        if protocol.is_null() {
            protocol = objc_allocateProtocol(name.as_ptr());
            if protocol.is_null() {
                return Err(format!("could not allocate protocol {name:?}"));
            }
            protocol_addMethodDescription(
                protocol,
                sel(c"isHandlingSendEvent"),
                BOOL_GETTER_TYPES.as_ptr(),
                1,
                1,
            );
            protocol_addMethodDescription(
                protocol,
                sel(c"setHandlingSendEvent:"),
                BOOL_SETTER_TYPES.as_ptr(),
                1,
                1,
            );
            objc_registerProtocol(protocol);
        }
        class_addProtocol(class, protocol);
        if class_conformsToProtocol(class, protocol) == 0 {
            return Err(format!("class does not conform to {name:?} after retrofit"));
        }
    }
    Ok(())
}
