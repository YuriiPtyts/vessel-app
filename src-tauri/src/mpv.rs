//! The only unsafe boundary. Each handle and its loaded library stay on one playback thread.
use std::{
    ffi::{CString, c_char, c_int, c_void},
    ptr::NonNull,
};

use libloading::Library;

use crate::{
    error::{AppError, Result},
    provider::StreamSource,
    runtime::RuntimePaths,
};

type Create = unsafe extern "C" fn() -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type SetString = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type Command = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type WaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const Event;
type Destroy = unsafe extern "C" fn(*mut c_void);

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

pub(crate) struct Mpv {
    handle: NonNull<c_void>,
    set_property: SetString,
    get_property: GetProperty,
    command: Command,
    wait_event: WaitEvent,
    destroy: Destroy,
    // Keeps all copied function pointers valid until after the handle is destroyed.
    _library: Library,
}

pub(crate) struct EngineEvents {
    pub loaded: bool,
    pub ended: bool,
    pub failed: bool,
}

impl Mpv {
    pub fn open(paths: &RuntimePaths) -> Result<Self> {
        let library_path = paths.require("libmpv.dylib")?;
        let unavailable = || {
            AppError::new(
                "engine_unavailable",
                "The bundled audio engine could not load. Rebuild native dependencies or reinstall the app.",
                false,
            )
        };
        // SAFETY: only the pinned bundled library is loaded. Symbol signatures match mpv's
        // public client API. The Library outlives every function pointer and native handle.
        let (
            library,
            handle,
            initialize,
            set_option,
            set_property,
            get_property,
            command,
            wait_event,
            destroy,
        ) = unsafe {
            let library = Library::new(library_path).map_err(|_| unavailable())?;
            let create = *library
                .get::<Create>(b"mpv_create\0")
                .map_err(|_| unavailable())?;
            let initialize = *library
                .get::<Initialize>(b"mpv_initialize\0")
                .map_err(|_| unavailable())?;
            let set_option = *library
                .get::<SetString>(b"mpv_set_option_string\0")
                .map_err(|_| unavailable())?;
            let set_property = *library
                .get::<SetString>(b"mpv_set_property_string\0")
                .map_err(|_| unavailable())?;
            let get_property = *library
                .get::<GetProperty>(b"mpv_get_property\0")
                .map_err(|_| unavailable())?;
            let command = *library
                .get::<Command>(b"mpv_command\0")
                .map_err(|_| unavailable())?;
            let wait_event = *library
                .get::<WaitEvent>(b"mpv_wait_event\0")
                .map_err(|_| unavailable())?;
            let destroy = *library
                .get::<Destroy>(b"mpv_terminate_destroy\0")
                .map_err(|_| unavailable())?;
            let handle = NonNull::new(create()).ok_or_else(unavailable)?;
            (
                library,
                handle,
                initialize,
                set_option,
                set_property,
                get_property,
                command,
                wait_event,
                destroy,
            )
        };
        let engine = Self {
            handle,
            set_property,
            get_property,
            command,
            wait_event,
            destroy,
            _library: library,
        };
        for (name, value) in [
            ("config", "no"),
            ("load-scripts", "no"),
            ("ytdl", "no"),
            ("vid", "no"),
            ("vo", "null"),
            ("audio-display", "no"),
            ("terminal", "no"),
            ("msg-level", "all=no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("cache", "yes"),
            ("demuxer-max-bytes", "16777216"),
            ("demuxer-max-back-bytes", "8388608"),
            ("network-timeout", "15"),
            ("gapless-audio", "yes"),
        ] {
            let name = cstring(name)?;
            let value = cstring(value)?;
            // SAFETY: the live handle and C strings are valid for this synchronous call.
            check(unsafe { set_option(engine.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })?;
        }
        // SAFETY: options were set on the newly created handle, on its owning thread.
        check(unsafe { initialize(engine.handle.as_ptr()) })?;
        Ok(engine)
    }

    pub fn load(
        &self,
        source: &StreamSource,
        position: f64,
        paused: bool,
        volume: f64,
    ) -> Result<()> {
        let headers = source
            .headers
            .iter()
            .map(|(name, value)| {
                // mpv's string-list syntax escapes commas/backslashes with a backslash.
                format!("{name}: {value}")
                    .replace('\\', "\\\\")
                    .replace(',', "\\,")
            })
            .collect::<Vec<_>>()
            .join(",");
        self.set("http-header-fields", &headers)?;
        self.set("start", &position.max(0.0).to_string())?;
        self.set("pause", if paused { "yes" } else { "no" })?;
        self.set("volume", &volume.to_string())?;
        self.run(&["loadfile", &source.url, "replace"])
    }

    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        let name = cstring(name)?;
        let value = cstring(value)?;
        // SAFETY: owned C strings and the handle live through this synchronous API call.
        check(unsafe { (self.set_property)(self.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })
    }

    pub fn run(&self, arguments: &[&str]) -> Result<()> {
        let strings: Vec<CString> = arguments
            .iter()
            .map(|arg| cstring(arg))
            .collect::<Result<_>>()?;
        let mut pointers: Vec<_> = strings.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(std::ptr::null());
        // SAFETY: mpv_command takes a null-terminated array and copies arguments before returning.
        check(unsafe { (self.command)(self.handle.as_ptr(), pointers.as_ptr()) })
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        let name = cstring(name).ok()?;
        let mut value = 0.0_f64;
        // SAFETY: MPV_FORMAT_DOUBLE (5) writes an f64 into the provided live, aligned storage.
        let status = unsafe {
            (self.get_property)(
                self.handle.as_ptr(),
                name.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            )
        };
        (status >= 0 && value.is_finite()).then_some(value)
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        let name = cstring(name).ok()?;
        let mut value = 0_i32;
        // SAFETY: MPV_FORMAT_FLAG (3) writes a C int into live, aligned storage.
        let status = unsafe {
            (self.get_property)(
                self.handle.as_ptr(),
                name.as_ptr(),
                3,
                (&mut value as *mut i32).cast(),
            )
        };
        (status >= 0).then_some(value != 0)
    }

    pub fn events(&self) -> EngineEvents {
        let mut events = EngineEvents {
            loaded: false,
            ended: false,
            failed: false,
        };
        for _ in 0..128 {
            // SAFETY: wait_event returns storage owned by the handle, valid until the next call.
            let event = unsafe { &*(self.wait_event)(self.handle.as_ptr(), 0.0) };
            match event.id {
                0 => break,                // MPV_EVENT_NONE
                8 => events.loaded = true, // MPV_EVENT_FILE_LOADED
                7 if !event.data.is_null() => {
                    // SAFETY: END_FILE has an mpv_event_end_file payload with this C layout.
                    let end = unsafe { &*event.data.cast::<EndFile>() };
                    events.failed |= end.reason == 4 || end.error < 0;
                    events.ended |= end.reason == 0;
                }
                _ => {}
            }
        }
        events
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: the actor owns the sole handle; no other thread can call it, and the library is still loaded.
        unsafe {
            (self.destroy)(self.handle.as_ptr());
        }
    }
}

fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| {
        AppError::new(
            "invalid_media_data",
            "The media source contains invalid native parameters.",
            false,
        )
    })
}

fn check(code: c_int) -> Result<()> {
    if code < 0 {
        Err(AppError::new(
            "engine_command",
            format!(
                "The audio engine rejected an operation (code {code}). Retry or choose another format."
            ),
            true,
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "loads bundled libmpv and reads actual native volume; no audio is played"]
    fn native_volume_values_are_applied_immediately() {
        let root = std::env::var_os("VESSEL_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
            });
        let engine = Mpv::open(&RuntimePaths::new(root)).unwrap();
        for volume in [0.0, 27.0, 64.0, 100.0, 0.0] {
            engine.set("volume", &volume.to_string()).unwrap();
            assert!((engine.number("volume").unwrap() - volume).abs() < 0.01);
        }
    }
}
