//! Runtime FFI bindings for libmpv (`mpv-1.dll` / `libmpv-2.dll`).

use std::ffi::{c_char, c_double, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::OnceLock;

use libloading::Library;

use crate::error::{Result, ViewerError};

pub(crate) type MpvHandle = *mut c_void;
pub(crate) type MpvRenderContext = *mut c_void;

pub(crate) const MPV_FORMAT_STRING: c_int = 1;
pub(crate) const MPV_FORMAT_FLAG: c_int = 3;
pub(crate) const MPV_FORMAT_INT64: c_int = 4;
pub(crate) const MPV_FORMAT_DOUBLE: c_int = 5;

pub(crate) const MPV_EVENT_NONE: c_int = 0;
pub(crate) const MPV_EVENT_SHUTDOWN: c_int = 1;

pub(crate) const MPV_RENDER_PARAM_INVALID: c_int = 0;
pub(crate) const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
pub(crate) const MPV_RENDER_PARAM_ADVANCED_CONTROL: c_int = 10;
// Values match include/mpv/render.h (SW_* sit after DRM_* params).
pub(crate) const MPV_RENDER_PARAM_SW_SIZE: c_int = 17;
pub(crate) const MPV_RENDER_PARAM_SW_FORMAT: c_int = 18;
pub(crate) const MPV_RENDER_PARAM_SW_STRIDE: c_int = 19;
pub(crate) const MPV_RENDER_PARAM_SW_POINTER: c_int = 20;

pub(crate) const MPV_RENDER_UPDATE_FRAME: u64 = 1 << 1;

#[repr(C)]
pub(crate) struct MpvEvent {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

#[repr(C)]
pub(crate) struct MpvRenderParam {
    pub type_: c_int,
    pub data: *mut c_void,
}

type FnCreate = unsafe extern "C" fn() -> MpvHandle;
type FnInitialize = unsafe extern "C" fn(MpvHandle) -> c_int;
type FnTerminateDestroy = unsafe extern "C" fn(MpvHandle);
type FnSetOptionString =
    unsafe extern "C" fn(MpvHandle, *const c_char, *const c_char) -> c_int;
type FnCommand = unsafe extern "C" fn(MpvHandle, *mut *const c_char) -> c_int;
type FnGetProperty = unsafe extern "C" fn(
    MpvHandle,
    *const c_char,
    c_int,
    *mut c_void,
) -> c_int;
type FnSetProperty = unsafe extern "C" fn(
    MpvHandle,
    *const c_char,
    c_int,
    *mut c_void,
) -> c_int;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnWaitEvent = unsafe extern "C" fn(MpvHandle, c_double) -> *mut MpvEvent;
type FnRenderContextCreate = unsafe extern "C" fn(
    *mut MpvRenderContext,
    MpvHandle,
    *mut MpvRenderParam,
) -> c_int;
type FnRenderContextFree = unsafe extern "C" fn(MpvRenderContext);
type FnRenderContextRender =
    unsafe extern "C" fn(MpvRenderContext, *mut MpvRenderParam) -> c_int;
type FnRenderContextUpdate = unsafe extern "C" fn(MpvRenderContext) -> u64;
type FnRenderContextSetUpdateCallback = unsafe extern "C" fn(
    MpvRenderContext,
    Option<unsafe extern "C" fn(*mut c_void)>,
    *mut c_void,
);
type FnErrorString = unsafe extern "C" fn(c_int) -> *const c_char;

pub(crate) struct MpvApi {
    _lib: Library,
    pub create: FnCreate,
    pub initialize: FnInitialize,
    pub terminate_destroy: FnTerminateDestroy,
    pub set_option_string: FnSetOptionString,
    pub command: FnCommand,
    pub get_property: FnGetProperty,
    pub set_property: FnSetProperty,
    pub free: FnFree,
    pub wait_event: FnWaitEvent,
    pub render_context_create: FnRenderContextCreate,
    pub render_context_free: FnRenderContextFree,
    pub render_context_render: FnRenderContextRender,
    pub render_context_update: FnRenderContextUpdate,
    pub render_context_set_update_callback: FnRenderContextSetUpdateCallback,
    pub error_string: FnErrorString,
}

static API: OnceLock<Result<MpvApi, String>> = OnceLock::new();

/// Whether a libmpv shared library can be loaded.
#[must_use]
pub fn mpv_available() -> bool {
    load_api().is_ok()
}

pub(crate) fn api() -> Result<&'static MpvApi> {
    load_api().map_err(|_| ViewerError::MediaUnavailable)
}

fn load_api() -> std::result::Result<&'static MpvApi, String> {
    let slot = API.get_or_init(|| match bind_mpv() {
        Ok(api) => Ok(api),
        Err(e) => Err(e),
    });
    match slot {
        Ok(api) => Ok(api),
        Err(e) => Err(e.clone()),
    }
}

fn bind_mpv() -> std::result::Result<MpvApi, String> {
    let mut last = String::from("libmpv not found");
    for path in candidate_library_paths() {
        match unsafe { load_from_path(&path) } {
            Ok(api) => {
                tracing::info!(path = %path.display(), "loaded libmpv");
                return Ok(api);
            }
            Err(e) => {
                last = format!("{}: {e}", path.display());
            }
        }
    }
    Err(last)
}

fn candidate_library_paths() -> Vec<PathBuf> {
    const NAMES: &[&str] = &["mpv-1.dll", "libmpv-2.dll", "mpv-2.dll", "libmpv.dll"];
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    // orchid-viewers crate → repo root third-party
    let viewers_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dirs.push(viewers_manifest.join("../../third-party/mpv/win-x64"));
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        dirs.push(PathBuf::from(manifest).join("../../third-party/mpv/win-x64"));
    }

    let mut paths = Vec::new();
    for dir in dirs {
        for name in NAMES {
            paths.push(dir.join(name));
        }
    }
    paths
}

unsafe fn load_from_path(path: &Path) -> std::result::Result<MpvApi, String> {
    if !path.is_file() {
        return Err("not a file".into());
    }
    let lib = Library::new(path).map_err(|e| e.to_string())?;
    unsafe {
        let create: FnCreate = *lib.get(b"mpv_create\0").map_err(|e| e.to_string())?;
        let initialize: FnInitialize =
            *lib.get(b"mpv_initialize\0").map_err(|e| e.to_string())?;
        let terminate_destroy: FnTerminateDestroy = *lib
            .get(b"mpv_terminate_destroy\0")
            .map_err(|e| e.to_string())?;
        let set_option_string: FnSetOptionString = *lib
            .get(b"mpv_set_option_string\0")
            .map_err(|e| e.to_string())?;
        let command: FnCommand = *lib.get(b"mpv_command\0").map_err(|e| e.to_string())?;
        let get_property: FnGetProperty =
            *lib.get(b"mpv_get_property\0").map_err(|e| e.to_string())?;
        let set_property: FnSetProperty =
            *lib.get(b"mpv_set_property\0").map_err(|e| e.to_string())?;
        let free: FnFree = *lib.get(b"mpv_free\0").map_err(|e| e.to_string())?;
        let wait_event: FnWaitEvent =
            *lib.get(b"mpv_wait_event\0").map_err(|e| e.to_string())?;
        let render_context_create: FnRenderContextCreate = *lib
            .get(b"mpv_render_context_create\0")
            .map_err(|e| e.to_string())?;
        let render_context_free: FnRenderContextFree = *lib
            .get(b"mpv_render_context_free\0")
            .map_err(|e| e.to_string())?;
        let render_context_render: FnRenderContextRender = *lib
            .get(b"mpv_render_context_render\0")
            .map_err(|e| e.to_string())?;
        let render_context_update: FnRenderContextUpdate = *lib
            .get(b"mpv_render_context_update\0")
            .map_err(|e| e.to_string())?;
        let render_context_set_update_callback: FnRenderContextSetUpdateCallback = *lib
            .get(b"mpv_render_context_set_update_callback\0")
            .map_err(|e| e.to_string())?;
        let error_string: FnErrorString =
            *lib.get(b"mpv_error_string\0").map_err(|e| e.to_string())?;
        Ok(MpvApi {
            _lib: lib,
            create,
            initialize,
            terminate_destroy,
            set_option_string,
            command,
            get_property,
            set_property,
            free,
            wait_event,
            render_context_create,
            render_context_free,
            render_context_render,
            render_context_update,
            render_context_set_update_callback,
            error_string,
        })
    }
}

pub(crate) fn c_str(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

pub(crate) unsafe fn error_message(api: &MpvApi, code: c_int) -> String {
    if code >= 0 {
        return String::new();
    }
    let ptr = (api.error_string)(code);
    if ptr.is_null() {
        return format!("mpv error {code}");
    }
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

pub(crate) unsafe fn get_double(api: &MpvApi, handle: MpvHandle, name: &str) -> Option<f64> {
    let key = c_str(name);
    let mut val: c_double = 0.0;
    let rc = (api.get_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_DOUBLE,
        (&raw mut val).cast(),
    );
    if rc >= 0 {
        Some(val)
    } else {
        None
    }
}

pub(crate) unsafe fn get_flag(api: &MpvApi, handle: MpvHandle, name: &str) -> Option<bool> {
    let key = c_str(name);
    let mut val: c_int = 0;
    let rc = (api.get_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_FLAG,
        (&raw mut val).cast(),
    );
    if rc >= 0 {
        Some(val != 0)
    } else {
        None
    }
}

pub(crate) unsafe fn get_int64(api: &MpvApi, handle: MpvHandle, name: &str) -> Option<i64> {
    let key = c_str(name);
    let mut val: i64 = 0;
    let rc = (api.get_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_INT64,
        (&raw mut val).cast(),
    );
    if rc >= 0 {
        Some(val)
    } else {
        None
    }
}

pub(crate) unsafe fn get_string(api: &MpvApi, handle: MpvHandle, name: &str) -> Option<String> {
    let key = c_str(name);
    let mut ptr: *mut c_char = ptr::null_mut();
    let rc = (api.get_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_STRING,
        (&raw mut ptr).cast(),
    );
    if rc < 0 || ptr.is_null() {
        return None;
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    (api.free)(ptr.cast());
    Some(s)
}

pub(crate) unsafe fn set_double(
    api: &MpvApi,
    handle: MpvHandle,
    name: &str,
    value: f64,
) -> c_int {
    let key = c_str(name);
    let mut val: c_double = value;
    (api.set_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_DOUBLE,
        (&raw mut val).cast(),
    )
}

pub(crate) unsafe fn set_flag(api: &MpvApi, handle: MpvHandle, name: &str, value: bool) -> c_int {
    let key = c_str(name);
    let mut val: c_int = i32::from(value);
    (api.set_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_FLAG,
        (&raw mut val).cast(),
    )
}

pub(crate) unsafe fn set_string(api: &MpvApi, handle: MpvHandle, name: &str, value: &str) -> c_int {
    let key = c_str(name);
    let val = c_str(value);
    let mut ptr: *const c_char = val.as_ptr();
    (api.set_property)(
        handle,
        key.as_ptr(),
        MPV_FORMAT_STRING,
        (&raw mut ptr).cast(),
    )
}

pub(crate) unsafe fn command_args(api: &MpvApi, handle: MpvHandle, args: &[&str]) -> c_int {
    let c_args: Vec<CString> = args.iter().map(|s| c_str(s)).collect();
    let mut ptrs: Vec<*const c_char> = c_args.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(ptr::null());
    (api.command)(handle, ptrs.as_mut_ptr())
}
