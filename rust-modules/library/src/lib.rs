//! Backend-neutral library browsing state for NTSMB.
//!
//! This crate consumes only the domain models in `ntsmb_core`. Network requests, persistence,
//! thumbnail decoding and UI navigation are effects owned by adapters; this module derives
//! visible rows and emits typed intents for the caller.

use std::collections::HashMap;
use ntsmb_core::{MediaFolder, MediaId, MediaItem, MediaType, PlaybackState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySort {
    #[default]
    Name,
    NameDescending,
    TypeThenName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaFilter {
    #[default]
    All,
    Folders,
    Video,
    Audio,
    Images,
}

impl MediaFilter {
    fn matches(self, item: &MediaItem) -> bool {
        match self {
            Self::All => true,
            Self::Folders => item.is_directory,
            Self::Video => item.media_type == MediaType::Video,
            Self::Audio => item.media_type == MediaType::Audio,
            Self::Images => item.media_type == MediaType::Image,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryIntent {
    OpenFolder { source_id: String, path: String },
    PlayMedia { source_id: String, path: String },
    None,
}

/// A view over one folder listing. Search, filtering and focus never mutate the source listing.
#[derive(Debug, Clone, Default)]
pub struct LibraryBrowser {
    folder: Option<MediaFolder>,
    query: String,
    filter: MediaFilter,
    sort: LibrarySort,
    selected: usize,
    progress: HashMap<MediaId, PlaybackState>,
}

impl LibraryBrowser {
    pub fn folder(&self) -> Option<&MediaFolder> { self.folder.as_ref() }
    pub fn query(&self) -> &str { &self.query }
    pub fn filter(&self) -> MediaFilter { self.filter }
    pub fn sort(&self) -> LibrarySort { self.sort }
    pub fn selected(&self) -> usize { self.selected }

    /// Replacing the listing for a different folder starts at its first visible item. Refreshing
    /// the same folder preserves the focused media identity even if rows are inserted, removed or
    /// reordered; if that item disappeared, the prior index is clamped to the new result count.
    pub fn set_folder(&mut self, folder: MediaFolder) {
        let changed_location = self.folder.as_ref().map_or(true, |old|
            old.source_id != folder.source_id || old.path != folder.path);
        let focused_id = if changed_location {
            None
        } else {
            self.selected_item().map(|item| item.id.clone())
        };
        self.folder = Some(folder);
        if changed_location {
            self.selected = 0;
        } else if let Some(focused_id) = focused_id {
            if let Some(index) = self.visible_items().iter().position(|item| item.id == focused_id) {
                self.selected = index;
            }
        }
        self.clamp_selection();
    }

    /// Clear stale rows when a backend listing cannot be converted safely.
    pub fn clear_folder(&mut self) {
        self.folder = None;
        self.selected = 0;
    }

    fn preserve_selection_while(&mut self, update: impl FnOnce(&mut Self)) {
        let selected_id = self.selected_item().map(|item| item.id.clone());
        update(self);
        if let Some(selected_id) = selected_id {
            if let Some(index) = self.visible_items().iter().position(|item| item.id == selected_id) {
                self.selected = index;
                return;
            }
        }
        self.clamp_selection();
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if self.query != query {
            self.preserve_selection_while(|browser| browser.query = query);
        }
    }

    pub fn clear_query(&mut self) { self.set_query(String::new()); }

    pub fn set_filter(&mut self, filter: MediaFilter) {
        if self.filter != filter {
            self.preserve_selection_while(|browser| browser.filter = filter);
        }
    }

    pub fn set_sort(&mut self, sort: LibrarySort) {
        if self.sort != sort {
            self.preserve_selection_while(|browser| browser.sort = sort);
        }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.visible_items().len().saturating_sub(1));
    }

    /// Visible rows follow the active filter/search and deterministic sort policy.
    pub fn visible_items(&self) -> Vec<&MediaItem> {
        let Some(folder) = self.folder.as_ref() else { return Vec::new(); };
        let needle = self.query.trim().to_lowercase();
        let mut rows: Vec<&MediaItem> = folder.items.iter()
            .filter(|item| self.filter.matches(item))
            .filter(|item| needle.is_empty() || item.name.to_lowercase().contains(&needle))
            .collect();
        rows.sort_by(|a, b| {
            let folders_first = b.is_directory.cmp(&a.is_directory);
            match self.sort {
                LibrarySort::Name => folders_first.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.name.cmp(&b.name)),
                LibrarySort::NameDescending => folders_first.then_with(|| b.name.to_lowercase().cmp(&a.name.to_lowercase()))
                    .then_with(|| b.name.cmp(&a.name)),
                LibrarySort::TypeThenName => folders_first.then_with(|| (a.media_type as u8).cmp(&(b.media_type as u8)))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.name.cmp(&b.name)),
            }
        });
        rows
    }

    pub fn selected_item(&self) -> Option<&MediaItem> {
        self.visible_items().get(self.selected).copied()
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = self.visible_items().len();
        if len == 0 { self.selected = 0; return; }
        self.selected = self.selected.saturating_add_signed(delta).min(len - 1);
    }

    pub fn select_index(&mut self, index: usize) {
        self.selected = index.min(self.visible_items().len().saturating_sub(1));
    }

    pub fn activate_selected(&self) -> LibraryIntent {
        let Some(folder) = self.folder.as_ref() else { return LibraryIntent::None; };
        let Some(item) = self.selected_item() else { return LibraryIntent::None; };
        if item.is_directory {
            LibraryIntent::OpenFolder { source_id: folder.source_id.clone(), path: item.path.clone() }
        } else if matches!(item.media_type, MediaType::Video | MediaType::Audio) {
            LibraryIntent::PlayMedia { source_id: folder.source_id.clone(), path: item.path.clone() }
        } else {
            LibraryIntent::None
        }
    }

    pub fn parent_intent(&self) -> Option<LibraryIntent> {
        let folder = self.folder.as_ref()?;
        let path = folder.parent_path.clone()?;
        Some(LibraryIntent::OpenFolder { source_id: folder.source_id.clone(), path })
    }

    /// Attach persisted progress supplied by a PlaybackStateRepository adapter.
    pub fn set_progress(&mut self, state: PlaybackState) {
        self.progress.insert(state.media_id.clone(), state);
    }

    pub fn progress_for(&self, media_id: &str) -> Option<&PlaybackState> {
        self.progress.get(media_id)
    }

    pub fn clear_progress(&mut self, media_id: &str) {
        self.progress.remove(media_id);
    }

    pub fn progress_percent(&self, media_id: &str) -> Option<u8> {
        let state = self.progress.get(media_id)?;
        if state.completed { return Some(100); }
        let duration = state.duration_ms?;
        if duration == 0 { return None; }
        Some(((state.position_ms.min(duration) as u128 * 100) / duration as u128) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, path: &str, directory: bool, media_type: MediaType) -> MediaItem {
        MediaItem { id: path.into(), name: name.into(), path: path.into(), size_bytes: None,
            is_directory: directory, media_type }
    }

    fn folder(path: &str) -> MediaFolder {
        MediaFolder { source_id: "nas".into(), path: path.into(),
            parent_path: Some("Films".into()), items: vec![
                item("zeta.mkv", &format!("{path}/zeta.mkv"), false, MediaType::Video),
                item("Extras", &format!("{path}/Extras"), true, MediaType::Folder),
                item("Alpha.mp4", &format!("{path}/Alpha.mp4"), false, MediaType::Video),
                item("cover.jpg", &format!("{path}/cover.jpg"), false, MediaType::Image),
            ] }
    }

    #[test]
    fn folders_sort_first_and_video_files_remain_playable() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        let rows = browser.visible_items();
        assert_eq!(rows.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
            vec!["Extras", "Alpha.mp4", "cover.jpg", "zeta.mkv"]);
        assert_eq!(browser.activate_selected(), LibraryIntent::OpenFolder {
            source_id: "nas".into(), path: "Films/Drama/Extras".into() });
        browser.move_selection(1);
        assert_eq!(browser.activate_selected(), LibraryIntent::PlayMedia {
            source_id: "nas".into(), path: "Films/Drama/Alpha.mp4".into() });
    }

    #[test]
    fn search_and_filter_reset_focus_without_mutating_source_listing() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        browser.move_selection(3);
        browser.set_query("ALPHA");
        assert_eq!(browser.visible_items().len(), 1);
        assert_eq!(browser.selected(), 0);
        assert_eq!(browser.folder().unwrap().items.len(), 4);
        browser.clear_query();
        browser.set_filter(MediaFilter::Video);
        assert_eq!(browser.visible_items().len(), 2);
    }

    #[test]
    fn changing_folder_resets_focus_and_invalid_listing_can_be_cleared() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        browser.select_index(3);
        browser.set_folder(folder("Films/Comedy"));
        assert_eq!(browser.selected(), 0);
        browser.clear_folder();
        assert!(browser.folder().is_none());
        assert!(browser.visible_items().is_empty());
        assert_eq!(browser.activate_selected(), LibraryIntent::None);
    }

    #[test]
    fn refresh_keeps_focus_on_the_same_media_when_rows_are_inserted() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        browser.select_index(3);
        assert_eq!(browser.selected_item().unwrap().name, "zeta.mkv");

        let mut refreshed = folder("Films/Drama");
        refreshed.items.push(item("Beta.mkv", "Films/Drama/Beta.mkv", false, MediaType::Video));
        browser.set_folder(refreshed);

        assert_eq!(browser.selected_item().unwrap().name, "zeta.mkv");
        assert_eq!(browser.selected(), 4, "the focused item moves with its identity, not its old row number");
    }

    #[test]
    fn sort_policy_is_deterministic_and_folders_stay_first() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        browser.set_sort(LibrarySort::NameDescending);
        assert_eq!(browser.visible_items()[0].name, "Extras");
        assert_eq!(browser.visible_items()[1].name, "zeta.mkv");
    }

    #[test]
    fn parent_navigation_uses_the_neutral_folder_contract() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder("Films/Drama"));
        assert_eq!(browser.parent_intent(), Some(LibraryIntent::OpenFolder {
            source_id: "nas".into(), path: "Films".into() }));
    }

    #[test]
    fn progress_percent_handles_missing_and_zero_duration_and_boundaries() {
        let mut browser = LibraryBrowser::default();
        browser.set_progress(PlaybackState { media_id: "unknown".into(), position_ms: 50,
            duration_ms: None, completed: false });
        assert_eq!(browser.progress_percent("unknown"), None);
        browser.set_progress(PlaybackState { media_id: "zero".into(), position_ms: 50,
            duration_ms: Some(0), completed: false });
        assert_eq!(browser.progress_percent("zero"), None);
        browser.set_progress(PlaybackState { media_id: "start".into(), position_ms: 0,
            duration_ms: Some(100), completed: false });
        assert_eq!(browser.progress_percent("start"), Some(0));
        browser.set_progress(PlaybackState { media_id: "middle".into(), position_ms: 55,
            duration_ms: Some(100), completed: false });
        assert_eq!(browser.progress_percent("middle"), Some(55));
    }

    #[test]
    fn progress_is_keyed_by_media_identity_and_clamped() {
        let mut browser = LibraryBrowser::default();
        browser.set_progress(PlaybackState { media_id: "film".into(), position_ms: 120,
            duration_ms: Some(100), completed: false });
        assert_eq!(browser.progress_percent("film"), Some(100));
        browser.set_progress(PlaybackState { media_id: "film".into(), position_ms: 50,
            duration_ms: Some(100), completed: true });
        assert_eq!(browser.progress_percent("film"), Some(100));
        browser.clear_progress("film");
        assert_eq!(browser.progress_for("film"), None);
    }
}


/// Core-contract bridge for saved playback progress.
pub mod progress;

/// Bounded opaque thumbnail storage; image decoding and fetching remain adapter-owned.
pub mod thumbnail_cache;

/// Stable identity-based focus save/restore helpers for screen adapters.
pub mod selection;


#[cfg(test)]
mod selection_policy_tests {
    use super::*;

    fn item(id: &str, name: &str, media_type: MediaType, is_directory: bool) -> MediaItem {
        MediaItem { id: id.into(), name: name.into(), path: name.into(),
            size_bytes: Some(1), is_directory, media_type }
    }

    fn browser() -> LibraryBrowser {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(MediaFolder {
            source_id: "smb".into(), path: String::new(), parent_path: None,
            items: vec![
                item("a", "Alpha.mkv", MediaType::Video, false),
                item("b", "Bravo.mp3", MediaType::Audio, false),
                item("c", "Charlie.mkv", MediaType::Video, false),
            ],
        });
        browser.select_media_id("b");
        browser
    }

    #[test]
    fn sorting_preserves_the_focused_item_identity() {
        let mut browser = browser();
        browser.set_sort(LibrarySort::NameDescending);
        assert_eq!(browser.selected_item().map(|item| item.id.as_str()), Some("b"));
    }

    #[test]
    fn filtering_preserves_focus_when_the_item_remains_visible() {
        let mut browser = browser();
        browser.set_filter(MediaFilter::Audio);
        assert_eq!(browser.selected_item().map(|item| item.id.as_str()), Some("b"));
    }

    #[test]
    fn filtering_out_the_focused_item_clamps_to_a_valid_visible_row() {
        let mut browser = browser();
        browser.set_filter(MediaFilter::Video);
        assert_eq!(browser.visible_items().len(), 2);
        assert!(browser.selected_item().is_some());
        assert_ne!(browser.selected_item().map(|item| item.id.as_str()), Some("b"));
    }
}
