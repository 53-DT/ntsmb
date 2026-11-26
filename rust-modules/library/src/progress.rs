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
    // Replace the requested IDs as one logical snapshot. Missing records clear stale in-memory
    // values, while a repository error leaves the browser completely unchanged.
    for media_id in media_ids {
        browser.clear_progress(media_id);
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

    #[derive(Default)]
    struct FailingProgress;

    impl PlaybackStateRepository for FailingProgress {
        fn load_state(&self, id: &str) -> Result<Option<PlaybackState>, String> {
            if id == "broken" { Err("storage unavailable".into()) } else { Ok(None) }
        }
        fn save_state(&self, _: &PlaybackState) -> Result<(), String> { Ok(()) }
        fn delete_state(&self, _: &str) -> Result<(), String> { Ok(()) }
    }

    #[test]
    fn restore_clears_missing_records_and_is_atomic_on_repository_error() {
        let mut browser = LibraryBrowser::default();
        browser.set_progress(state("stale", 100));
        restore_progress(&mut browser, &MemoryProgress::default(), &["stale".into()]).unwrap();
        assert!(browser.progress_for("stale").is_none(),
            "a missing persisted record must not leave stale in-memory progress");

        browser.set_progress(state("keep", 250));
        assert!(restore_progress(&mut browser, &FailingProgress,
            &["keep".into(), "broken".into()]).is_err());
        assert_eq!(browser.progress_for("keep"), Some(&state("keep", 250)),
            "a failed restore must not partially alter the browser");
    }


    #[derive(Default)]
    struct FailingWrites;

    impl PlaybackStateRepository for FailingWrites {
        fn load_state(&self, _: &str) -> Result<Option<PlaybackState>, String> { Ok(None) }
        fn save_state(&self, _: &PlaybackState) -> Result<(), String> { Err("disk full".into()) }
        fn delete_state(&self, _: &str) -> Result<(), String> { Err("read only".into()) }
    }

    #[test]
    fn failed_persist_and_delete_leave_the_library_snapshot_unchanged() {
        let mut browser = LibraryBrowser::default();
        browser.set_progress(state("film", 420));
        let before = browser.progress_for("film").cloned();

        assert_eq!(persist_progress(&browser, &FailingWrites, "film"),
            Err("disk full".into()));
        assert_eq!(browser.progress_for("film"), before.as_ref(),
            "a failed save must not mutate the in-memory snapshot");

        assert_eq!(delete_progress(&mut browser, &FailingWrites, "film"),
            Err("read only".into()));
        assert_eq!(browser.progress_for("film"), before.as_ref(),
            "a failed delete must retain the in-memory state so it can be retried");
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
