//! Muted native tests use locally generated silence, without network or audible audio.
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
