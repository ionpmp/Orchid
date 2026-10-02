//! Windows touch/pen classification and system tap feedback.

use orchid_core::ContactKind;

/// `PT_PEN` when Windows still knows this pointer id; otherwise a finger.
#[must_use]
pub fn contact_kind(pointer_id: u64) -> ContactKind {
    #[cfg(windows)]
    {
        if pen_pointer(pointer_id) {
            return ContactKind::Pen;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pointer_id;
    }
    ContactKind::Finger
}

/// Keep or suppress Windows touch and pen tap feedback on this window.
pub fn apply(window: &slint::Window, enabled: bool) {
    #[cfg(windows)]
    {
        apply_windows(window, enabled);
    }
    #[cfg(not(windows))]
    {
        let _ = (window, enabled);
    }
}

#[cfg(windows)]
fn pen_pointer(pointer_id: u64) -> bool {
    use windows::Win32::UI::Input::Pointer::GetPointerType;
    use windows::Win32::UI::WindowsAndMessaging::{POINTER_INPUT_TYPE, PT_PEN};
    let mut kind = POINTER_INPUT_TYPE::default();
    unsafe { GetPointerType(pointer_id as u32, &mut kind) }.is_ok() && kind == PT_PEN
}

#[cfg(windows)]
fn apply_windows(window: &slint::Window, enabled: bool) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::WinitWindowAccessor;
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Controls::{
        SetWindowFeedbackSetting, FEEDBACK_PEN_DOUBLETAP, FEEDBACK_PEN_TAP,
        FEEDBACK_TOUCH_DOUBLETAP, FEEDBACK_TOUCH_TAP,
    };

    let Some(bits) = window.with_winit_window(|winit_window| {
        let handle = winit_window.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        }
    }) else {
        return;
    };
    let Some(bits) = bits else {
        return;
    };
    if bits == 0 {
        return;
    }
    let hwnd = HWND(bits as *mut _);
    let flag = BOOL::from(enabled);
    let size = std::mem::size_of::<BOOL>() as u32;
    for feedback in [
        FEEDBACK_TOUCH_TAP,
        FEEDBACK_TOUCH_DOUBLETAP,
        FEEDBACK_PEN_TAP,
        FEEDBACK_PEN_DOUBLETAP,
    ] {
        unsafe {
            let _ = SetWindowFeedbackSetting(
                hwnd,
                feedback,
                0,
                size,
                Some((&flag as *const BOOL).cast()),
            );
        }
    }
}
