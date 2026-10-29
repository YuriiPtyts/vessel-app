//! Selected browsing/playback service; credentials and queues remain per provider.
use crate::{
    account::AccountSnapshot,
    application::Application,
    error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MusicProvider {
    #[serde(rename = "youtubeMusic")]
    YouTubeMusic = 0,
    #[serde(rename = "soundcloud")]
    SoundCloud = 1,
}
impl MusicProvider {
    pub fn from_gate(gate: &AtomicU8) -> Self {
        if gate.load(Ordering::SeqCst) == 1 {
            Self::SoundCloud
        } else {
            Self::YouTubeMusic
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub active: MusicProvider,
    pub revision: u64,
    pub youtube: AccountSnapshot,
    pub soundcloud: AccountSnapshot,
}
pub struct MusicServices {
    youtube: Arc<Application>,
    soundcloud: Arc<Application>,
    gate: Arc<AtomicU8>,
    revision: AtomicU64,
    operation: Mutex<()>,
    path: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Preference {
    version: u8,
    active: MusicProvider,
}
impl MusicServices {
    pub fn initial(path: &Path) -> MusicProvider {
        std::fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.len() < 65_536)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Preference>(&bytes).ok())
            .filter(|preference| preference.version == 1)
            .map_or(MusicProvider::YouTubeMusic, |preference| preference.active)
    }
    pub fn new(
        youtube: Arc<Application>,
        soundcloud: Arc<Application>,
        gate: Arc<AtomicU8>,
        path: Option<PathBuf>,
    ) -> Arc<Self> {
        youtube.set_foreground(MusicProvider::from_gate(&gate) == MusicProvider::YouTubeMusic);
        soundcloud.set_foreground(MusicProvider::from_gate(&gate) == MusicProvider::SoundCloud);
        Arc::new(Self {
            youtube,
            soundcloud,
            gate,
            revision: AtomicU64::new(0),
            operation: Mutex::new(()),
            path,
        })
    }
    pub fn active(&self) -> Arc<Application> {
        self.app(MusicProvider::from_gate(&self.gate))
    }
    pub fn app(&self, provider: MusicProvider) -> Arc<Application> {
        match provider {
            MusicProvider::YouTubeMusic => self.youtube.clone(),
            MusicProvider::SoundCloud => self.soundcloud.clone(),
        }
    }
    pub fn snapshot(&self) -> ProviderSnapshot {
        ProviderSnapshot {
            active: MusicProvider::from_gate(&self.gate),
            revision: self.revision.load(Ordering::SeqCst),
            youtube: self.youtube.account(),
            soundcloud: self.soundcloud.account(),
        }
    }
    pub async fn switch(&self, provider: MusicProvider) -> Result<ProviderSnapshot> {
        let _operation = self.operation.lock().await;
        if self.snapshot().active == provider {
            return Ok(self.snapshot());
        }
        let outgoing = self.active();
        if let Err(error) = outgoing.deactivate().await {
            outgoing.activate_provider();
            return Err(error);
        }
        if let Some(path) = &self.path {
            let result = (|| {
                let parent = path.parent().ok_or_else(AppError::internal)?;
                std::fs::create_dir_all(parent).map_err(|_| mode_error())?;
                let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| mode_error())?;
                file.write_all(
                    &serde_json::to_vec(&Preference {
                        version: 1,
                        active: provider,
                    })
                    .map_err(|_| mode_error())?,
                )
                .map_err(|_| mode_error())?;
                file.persist(path).map_err(|_| mode_error())?;
                Ok::<_, AppError>(())
            })();
            if let Err(error) = result {
                outgoing.activate_provider();
                return Err(error);
            }
        }
        let incoming = self.app(provider);
        incoming.volume(outgoing.snapshot()?.volume).await?;
        incoming.activate_provider();
        self.gate.store(provider as u8, Ordering::SeqCst);
        self.revision.fetch_add(1, Ordering::SeqCst);
        Ok(self.snapshot())
    }
    pub async fn restore_sessions(&self) {
        tokio::join!(
            self.youtube.restore_session(),
            self.soundcloud.restore_session()
        );
    }
    pub fn shutdown(&self) {
        self.youtube.shutdown();
        self.soundcloud.shutdown();
    }
}
fn mode_error() -> AppError {
    AppError::new(
        "provider_preference",
        "The selected music service could not be saved. Try switching again.",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn switching_selects_independent_contexts_and_stores_only_the_service() {
        let directory = tempfile::tempdir().unwrap();
        let paths = crate::runtime::RuntimePaths::new(directory.path().join("unavailable-runtime"));
        let youtube = Application::new(paths.clone(), |_| {}).unwrap();
        let cloud = Application::soundcloud_desktop(
            paths,
            Arc::new(crate::library::LocalLibrary::memory()),
            directory.path().join("cloud-queue.json"),
            true,
            |_| {},
        )
        .unwrap();
        let path = directory.path().join("music-service.json");
        let services = MusicServices::new(
            youtube.clone(),
            cloud.clone(),
            Arc::new(AtomicU8::new(0)),
            Some(path.clone()),
        );
        let before = youtube.account();
        let selected = services.switch(MusicProvider::SoundCloud).await.unwrap();
        assert_eq!(selected.active, MusicProvider::SoundCloud);
        assert!(Arc::ptr_eq(&services.active(), &cloud));
        assert_eq!(selected.youtube, before);
        assert_eq!(MusicServices::initial(&path), MusicProvider::SoundCloud);
        let back = services.switch(MusicProvider::YouTubeMusic).await.unwrap();
        assert_eq!(back.youtube, before);
        assert_eq!(back.soundcloud.mode, crate::account::AccountMode::Guest);
        assert!(back.revision > selected.revision);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("fixture-youtube"));
        std::fs::write(&path, b"malformed preference").unwrap();
        assert_eq!(MusicServices::initial(&path), MusicProvider::YouTubeMusic);
        assert_eq!(std::fs::read(&path).unwrap(), b"malformed preference");
        services.shutdown();
    }
}
