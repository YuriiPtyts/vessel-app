//! Local native fixtures. Tone tests require the output-silencing render audit.
use crate::{
    provider::{AudioFormat, StreamSource, Track},
    runtime::RuntimePaths,
};
use std::{io::Write, path::PathBuf};

pub fn paths() -> RuntimePaths {
    RuntimePaths::new(
        std::env::var_os("VESSEL_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")),
    )
}
pub fn silence() -> tempfile::NamedTempFile {
    silence_at(48000, 1)
}
pub fn silence_at(rate: u32, channels: u16) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    let frame_bytes = channels * 2;
    let bytes = rate * u32::from(frame_bytes) * 30;
    let mut header = Vec::new();
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(bytes + 36).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16_u32.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&rate.to_le_bytes());
    header.extend_from_slice(&(rate * u32::from(frame_bytes)).to_le_bytes());
    header.extend_from_slice(&frame_bytes.to_le_bytes());
    header.extend_from_slice(&16_u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&bytes.to_le_bytes());
    file.write_all(&header).unwrap();
    file.write_all(&vec![0; bytes as usize]).unwrap();
    file
}

#[cfg(target_os = "macos")]
pub fn render_probe_tone() -> tempfile::NamedTempFile {
    use std::io::{Seek, SeekFrom};
    let rate = 44_100;
    let mut file = silence_at(rate, 2);
    let mut samples = Vec::with_capacity((rate * 30 * 4) as usize);
    for frame in 0..rate * 30 {
        let sample = ((f64::from(frame) * 1000.0 * std::f64::consts::TAU / f64::from(rate)).sin()
            * 4096.0) as i16;
        samples.extend_from_slice(&sample.to_le_bytes());
        samples.extend_from_slice(&sample.to_le_bytes());
    }
    file.as_file_mut().seek(SeekFrom::Start(44)).unwrap();
    file.write_all(&samples).unwrap();
    file
}

#[cfg(target_os = "macos")]
pub struct RenderAudit {
    _library: libloading::Library,
    peak: unsafe extern "C" fn() -> f64,
    blocks: unsafe extern "C" fn() -> u64,
    frames: unsafe extern "C" fn() -> u32,
    generations: unsafe extern "C" fn() -> u64,
    first_gain: unsafe extern "C" fn() -> f64,
}

#[cfg(target_os = "macos")]
impl RenderAudit {
    pub fn new() -> Self {
        let library: libloading::Library = libloading::os::unix::Library::this().into();
        // SAFETY: only the explicit fixture image exports these atomic observers.
        // Resolve all symbols before a tone can start; keep the library alive.
        unsafe {
            Self {
                peak: *library.get(b"vessel_volume_render_peak\0").unwrap(),
                blocks: *library.get(b"vessel_volume_render_blocks\0").unwrap(),
                frames: *library.get(b"vessel_volume_render_frames\0").unwrap(),
                generations: *library.get(b"vessel_volume_output_generations\0").unwrap(),
                first_gain: *library.get(b"vessel_volume_output_first_gain\0").unwrap(),
                _library: library,
            }
        }
    }
    pub fn peak(&self) -> f64 {
        // SAFETY: the fixture's library outlives this atomic observer.
        unsafe { (self.peak)() }
    }
    pub fn blocks(&self) -> u64 {
        // SAFETY: the fixture's library outlives this atomic observer.
        unsafe { (self.blocks)() }
    }
    pub fn frames(&self) -> u32 {
        // SAFETY: the fixture's library outlives this atomic observer.
        unsafe { (self.frames)() }
    }
    pub fn generations(&self) -> u64 {
        // SAFETY: the fixture's library outlives this atomic observer.
        unsafe { (self.generations)() }
    }
    pub fn first_gain(&self) -> f64 {
        // SAFETY: the fixture's library outlives this atomic observer.
        unsafe { (self.first_gain)() }
    }
}
pub fn selection(file: &tempfile::NamedTempFile) -> (Track, AudioFormat, StreamSource) {
    (
        Track {
            id: "khnokW3Mw24".into(),
            title: "Silent native fixture".into(),
            artist: "Fixture".into(),
            duration_seconds: Some(30.0),
            album: None,
            thumbnail_url: None,
        },
        AudioFormat {
            id: "pcm".into(),
            codec: "pcm_s16le".into(),
            container: "wav".into(),
            bitrate_kbps: Some(768.0),
            sample_rate_hz: Some(48000),
            channels: Some(1),
        },
        crate::provider::test_source(file.path().to_str().unwrap()),
    )
}
