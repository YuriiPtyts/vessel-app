use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct RuntimePaths {
    pub root: PathBuf,
}

impl RuntimePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn available(&self) -> bool {
        ["libmpv.dylib", "extractor/yt-dlp_macos", "deno"]
            .iter()
            .all(|name| self.root.join(name).is_file())
    }

    pub fn require(&self, name: &str) -> Result<PathBuf> {
        let path = self.root.join(if name == "yt-dlp" {
            "extractor/yt-dlp_macos"
        } else {
            name
        });
        if !path.is_file() {
            return Err(AppError::new(
                "runtime_missing",
                "The audio engine is missing from this build. Reinstall Vessel or rebuild the app.",
                false,
            ));
        }
        Ok(path)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
