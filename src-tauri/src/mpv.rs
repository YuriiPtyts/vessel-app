//! Playback's unsafe boundary. Each handle and its loaded library stay on one playback thread.
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
        let library_path = paths.require("libmpv")?;
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
            ("audio-client-name", "Vessel"),
            ("terminal", "no"),
            ("msg-level", "all=no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("volume", "100"),
            ("cache", "yes"),
            ("demuxer-max-bytes", "16777216"),
            ("demuxer-max-back-bytes", "8388608"),
            ("network-timeout", "15"),
            ("gapless-audio", "yes"),
            ("audio-buffer", "0.05"),
        ] {
            let name = cstring(name)?;
            let value = cstring(value)?;
            // SAFETY: the live handle and C strings are valid for this synchronous call.
            check(unsafe { set_option(engine.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })?;
        }
        #[cfg(target_os = "windows")]
        {
            let name = cstring("ao")?;
            let value = cstring("wasapi")?;
            // SAFETY: a bundled audio-only Windows handle uses the system WASAPI backend.
            check(unsafe { set_option(engine.handle.as_ptr(), name.as_ptr(), value.as_ptr()) })?;
        }
        #[cfg(all(test, target_os = "macos"))]
        if let Ok(backend) = std::env::var("VESSEL_VOLUME_PROBE_BACKEND") {
            assert!(matches!(backend.as_str(), "coreaudio" | "avfoundation"));
            let name = cstring("ao")?;
            let value = cstring(&backend)?;
            // SAFETY: fixture-only backend selection before native initialization.
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
        self.set_volume(volume)?;
        self.run(&["loadfile", &source.url, "replace"])
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            // Keep the established perceptual curve, but apply it after queued PCM,
            // in the per-player AudioUnit/AVSampleBufferAudioRenderer output gain.
            let gain = (volume / 100.0).powi(3) * 100.0;
            self.set("vessel-output-volume", &gain.to_string())
        }
        #[cfg(not(target_os = "macos"))]
        self.set("volume", &volume.to_string())
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

    #[cfg(test)]
    fn text(&self, name: &str) -> Option<String> {
        let name = cstring(name).ok()?;
        let mut value = std::ptr::null_mut::<c_char>();
        // SAFETY: MPV_FORMAT_STRING writes an allocated char pointer. It is copied then freed with mpv_free.
        unsafe {
            let free = self
                ._library
                .get::<unsafe extern "C" fn(*mut c_void)>(b"mpv_free\0")
                .ok()?;
            let status = (self.get_property)(
                self.handle.as_ptr(),
                name.as_ptr(),
                1,
                (&mut value as *mut *mut c_char).cast(),
            );
            if status < 0 || value.is_null() {
                return None;
            }
            let text = std::ffi::CStr::from_ptr(value)
                .to_string_lossy()
                .into_owned();
            free(value.cast());
            Some(text)
        }
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

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the local CoreAudio initialization-failure audit library"]
    fn native_failed_audio_init_releases_device_callbacks() {
        use std::time::{Duration, Instant};
        // SAFETY: the explicit diagnostic library exports this no-argument counter.
        let audit = unsafe {
            libloading::os::unix::Library::this()
                .get::<unsafe extern "C" fn() -> i32>(b"vessel_audio_listener_count\0")
                .unwrap()
        };
        let engine = Mpv::open(&crate::test_audio::paths()).unwrap();
        // SAFETY: the audit function is provided by the explicit test image.
        let baseline = unsafe { audit() };
        let wav = crate::test_audio::silence_at(44100, 2);
        engine.set("volume", "0").unwrap();
        engine.set("ao", "coreaudio,avfoundation").unwrap();
        engine
            .run(&["loadfile", wav.path().to_str().unwrap(), "replace"])
            .unwrap();
        let start = Instant::now();
        while engine.text("current-ao").is_none() {
            assert!(start.elapsed() < Duration::from_secs(5));
            assert!(!engine.events().failed);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(engine.text("current-ao").as_deref(), Some("avfoundation"));
        // SAFETY: the counter is valid while the diagnostic image remains loaded.
        let outstanding = unsafe { audit() };
        println!("CoreAudio callbacks after failed init and AVFoundation fallback: {outstanding}");
        assert_eq!(
            outstanding, baseline,
            "a failed audio driver left CoreAudio callbacks referencing its released context"
        );
        engine.run(&["stop"]).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "real muted native decoding and temporary aggregate-device publish/remove; no network"]
    fn native_audio_survives_device_publication_and_removal() {
        use std::time::{Duration, Instant};
        let engine = Mpv::open(&crate::test_audio::paths()).unwrap();
        let channels = if std::env::var("VESSEL_AUDIO_MONO").as_deref() == Ok("1") {
            1
        } else {
            2
        };
        let wav = crate::test_audio::silence_at(44100, channels);
        engine.set("volume", "0").unwrap();
        engine.set("ao", "coreaudio,avfoundation").unwrap();
        engine.set("loop-file", "inf").unwrap();
        engine
            .run(&["loadfile", wav.path().to_str().unwrap(), "replace"])
            .unwrap();
        let start = Instant::now();
        while engine.number("audio-params/samplerate").is_none()
            || engine
                .number("time-pos")
                .is_none_or(|position| position < 0.1)
        {
            assert!(!engine.events().failed);
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        println!(
            "Native hotplug output: {:?}; format {:?}",
            engine.text("current-ao"),
            engine.text("audio-out-params/format")
        );
        let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../scripts/audio-hotplug.swift");
        let mut trigger = std::process::Command::new("/usr/bin/swift");
        trigger
            .arg(script)
            .arg("--switch-default")
            .env_remove("DYLD_INSERT_LIBRARIES");
        if std::env::var("VESSEL_AUDIO_STRESS").as_deref() == Ok("1") {
            trigger.arg("--stress");
        }
        let status = trigger.status().unwrap();
        assert!(status.success(), "native hotplug trigger failed");
        std::thread::sleep(Duration::from_millis(300));
        assert!(!engine.events().failed);
        assert!(engine.number("audio-params/samplerate").is_some());
        assert!(
            engine
                .number("time-pos")
                .is_some_and(|position| position > 0.1)
        );
        assert!(engine.text("current-ao").is_some());
        println!(
            "Native output after hotplug: {:?}",
            engine.text("current-ao")
        );
        let resumed_position = engine.number("time-pos").unwrap();
        let recovery = Instant::now();
        while engine
            .number("time-pos")
            .is_none_or(|position| position <= resumed_position + 0.05)
        {
            assert!(!engine.events().failed);
            assert!(
                recovery.elapsed() < Duration::from_secs(3),
                "native playback did not advance after output restoration: position {resumed_position:?} -> {:?}",
                engine.number("time-pos")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        println!("Native output resumed in {:?}", recovery.elapsed());
        engine.run(&["stop"]).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "requires the output-silencing render audit; run scripts/check-native-volume.mjs"]
    fn native_output_gain_is_restored_before_recreated_output_starts() {
        use std::time::{Duration, Instant};
        let audit = crate::test_audio::RenderAudit::new();
        let engine = Mpv::open(&crate::test_audio::paths()).unwrap();
        let audio = crate::test_audio::render_probe_tone();
        let (track, _, source) = crate::test_audio::selection(&audio);
        engine.load(&source, 0.0, false, 0.0).unwrap();
        for volume in [0.0_f64, 27.0, 73.0, 0.0] {
            engine.set_volume(volume).unwrap();
            let previous = audit.generations();
            let previous_blocks = audit.blocks();
            engine.run(&["ao-reload"]).unwrap();
            let start = Instant::now();
            while audit.generations() <= previous || audit.blocks() <= previous_blocks + 5 {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "Output was not recreated for {}",
                    track.title
                );
                assert!(!engine.events().failed);
                std::thread::sleep(Duration::from_millis(2));
            }
            let gain = (volume / 100.0).powi(3);
            assert!(
                (audit.first_gain() - gain).abs() < 0.0001,
                "New output started with the wrong gain: {} vs {gain}",
                audit.first_gain()
            );
            assert!((engine.number("ao-volume").unwrap() / 100.0 - gain).abs() < 0.0001);
            assert_eq!(engine.number("volume"), Some(100.0));
        }
        engine.run(&["stop"]).unwrap();
    }

    #[test]
    #[ignore = "loads bundled libmpv and checks the retained output target and curve; no audio is played"]
    fn native_volume_values_are_applied_immediately() {
        let root = std::env::var_os("VESSEL_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")
            });
        let engine = Mpv::open(&RuntimePaths::new(root)).unwrap();
        for volume in [0.0_f64, 27.0, 64.0, 100.0, 0.0] {
            engine.set_volume(volume).unwrap();
            #[cfg(target_os = "macos")]
            {
                let expected = (volume / 100.0).powi(3) * 100.0;
                assert!((engine.number("vessel-output-volume").unwrap() - expected).abs() < 0.0001);
                assert_eq!(engine.number("volume"), Some(100.0));
            }
            #[cfg(not(target_os = "macos"))]
            assert!((engine.number("volume").unwrap() - volume).abs() < 0.0001);
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "loads the packaged Windows DLL and decodes local PCM to a null output"]
    fn native_windows_runtime_decodes_local_audio() {
        use std::time::{Duration, Instant};
        let engine = Mpv::open(&crate::test_audio::paths()).unwrap();
        let configuration = engine.text("mpv-configuration").unwrap();
        assert!(configuration.contains("gpl=false") || configuration.contains("--disable-gpl"));
        assert!(engine.text("mpv-version").unwrap().contains("mpv"));
        engine.set("ao", "null").unwrap();
        let audio = crate::test_audio::silence();
        let (_, _, source) = crate::test_audio::selection(&audio);
        engine.load(&source, 0.0, false, 0.0).unwrap();
        let start = Instant::now();
        while engine
            .number("time-pos")
            .is_none_or(|position| position < 0.2)
        {
            assert!(!engine.events().failed);
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(engine.number("audio-params/samplerate").is_some());
        engine.run(&["stop"]).unwrap();
    }
}
