//! Backend-neutral library browsing state for NTSMB.
//!
//! This crate consumes only the domain models in `ntsmb_core`. Network requests and UI
//! navigation are effects owned by the application; this module only derives visible rows and
//! emits typed intents for the caller to execute.

use ntsmb_core::{MediaFolder, MediaItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryIntent {
    OpenFolder { source_id: String, path: String },
    PlayMedia { source_id: String, path: String },
    None,
}

/// A view over one folder listing. Search and focus never mutate the source listing.
#[derive(Debug, Clone, Default)]
pub struct LibraryBrowser {
    folder: Option<MediaFolder>,
    query: String,
    selected: usize,
}

impl LibraryBrowser {
    pub fn folder(&self) -> Option<&MediaFolder> { self.folder.as_ref() }
    pub fn query(&self) -> &str { &self.query }
    pub fn selected(&self) -> usize { self.selected }

    pub fn set_folder(&mut self, folder: MediaFolder) {
        self.folder = Some(folder);
        self.selected = self.selected.min(self.visible_items().len().saturating_sub(1));
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.selected = self.selected.min(self.visible_items().len().saturating_sub(1));
    }

    pub fn clear_query(&mut self) { self.set_query(String::new()); }

    /// Visible rows are folders first, then case-insensitive alphabetical order.
    pub fn visible_items(&self) -> Vec<&MediaItem> {
        let Some(folder) = self.folder.as_ref() else { return Vec::new(); };
        let needle = self.query.trim().to_lowercase();
        let mut rows: Vec<&MediaItem> = folder.items.iter()
            .filter(|item| needle.is_empty() || item.name.to_lowercase().contains(&needle))
            .collect();
        rows.sort_by(|a, b| b.is_directory.cmp(&a.is_directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name)));
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

    pub fn activate_selected(&self) -> LibraryIntent {
        let Some(folder) = self.folder.as_ref() else { return LibraryIntent::None; };
        let Some(item) = self.selected_item() else { return LibraryIntent::None; };
        if item.is_directory {
            LibraryIntent::OpenFolder { source_id: folder.source_id.clone(), path: item.path.clone() }
        } else if matches!(item.media_type, ntsmb_core::MediaType::Video | ntsmb_core::MediaType::Audio) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntsmb_core::MediaType;

    fn item(name: &str, path: &str, directory: bool, media_type: MediaType) -> MediaItem {
        MediaItem { id: path.into(), name: name.into(), path: path.into(), size_bytes: None,
            is_directory: directory, media_type }
    }

    fn folder() -> MediaFolder {
        MediaFolder { source_id: "nas".into(), path: "Films/Drama".into(),
            parent_path: Some("Films".into()), items: vec![
                item("zeta.mkv", "Films/Drama/zeta.mkv", false, MediaType::Video),
                item("Extras", "Films/Drama/Extras", true, MediaType::Folder),
                item("Alpha.mp4", "Films/Drama/Alpha.mp4", false, MediaType::Video),
                item("cover.jpg", "Films/Drama/cover.jpg", false, MediaType::Image),
            ] }
    }

    #[test]
    fn folders_sort_first_and_video_files_remain_playable() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder());
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
    fn search_filters_without_mutating_the_folder_and_clamps_focus() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder());
        browser.move_selection(3);
        browser.set_query("ALPHA");
        assert_eq!(browser.visible_items().len(), 1);
        assert_eq!(browser.selected(), 0);
        assert_eq!(browser.folder().unwrap().items.len(), 4);
    }

    #[test]
    fn parent_navigation_uses_the_neutral_folder_contract() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder());
        assert_eq!(browser.parent_intent(), Some(LibraryIntent::OpenFolder {
            source_id: "nas".into(), path: "Films".into() }));
    }
}
