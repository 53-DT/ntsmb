//! Persistence bridge for the backend-neutral library's watch progress.
use ntsmb_core::{MediaId, PlaybackStateRepository};
use crate::LibraryBrowser;

/// Load a group of saved progress records into the library view model.
/// The browser is updated only after every requested repository read succeeds.
pub fn restore_progress<R: PlaybackStateRepository>(
    browser: &mut LibraryBrowser,
    repository: &R,
    media_ids: &[MediaId],
) -> Result<(), String> {
    let mut loaded = Vec::new();
    for media_id in media_ids {
        if let Some(state) = repository.load_state(media_id)? {
            loaded.push(state);
        }
    }
    for state in loaded {
        browser.set_progress(state);
    }
    Ok(())
}

/// Persist the current progress record for one media item when present.
pub fn persist_progress<R: PlaybackStateRepository>(
    browser: &LibraryBrowser,
    repository: &R,
    media_id: &str,
) -> Result<bool, String> {
    let Some(state) = browser.progress_for(media_id) else { return Ok(false); };
    repository.save_state(state)?;
    Ok(true)
}

/// Delete persisted and in-memory progress together.
pub fn delete_progress<R: PlaybackStateRepository>(
    browser: &mut LibraryBrowser,
    repository: &R,
    media_id: &str,
) -> Result<(), String> {
    repository.delete_state(media_id)?;
    browser.clear_progress(media_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntsmb_core::PlaybackState;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct MemoryProgress(Mutex<HashMap<MediaId, PlaybackState>>);

    impl PlaybackStateRepository for MemoryProgress {
        fn load_state(&self, id: &str) -> Result<Option<PlaybackState>, String> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn save_state(&self, state: &PlaybackState) -> Result<(), String> {
            self.0.lock().unwrap().insert(state.media_id.clone(), state.clone());
            Ok(())
        }
        fn delete_state(&self, id: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }

    fn state(id: &str, position: u64) -> PlaybackState {
        PlaybackState { media_id: id.into(), position_ms: position,
            duration_ms: Some(1_000), completed: false }
    }

    #[test]
    fn restore_save_and_delete_use_only_the_core_repository_contract() {
        let repository = MemoryProgress::default();
        let mut browser = LibraryBrowser::default();
        repository.save_state(&state("film", 420)).unwrap();
        restore_progress(&mut browser, &repository, &["film".into(), "missing".into()]).unwrap();
        assert_eq!(browser.progress_for("film"), Some(&state("film", 420)));
        assert!(persist_progress(&browser, &repository, "film").unwrap());
        delete_progress(&mut browser, &repository, "film").unwrap();
        assert!(browser.progress_for("film").is_none());
        assert_eq!(repository.load_state("film").unwrap(), None);
        assert!(!persist_progress(&browser, &repository, "film").unwrap());
    }
}
