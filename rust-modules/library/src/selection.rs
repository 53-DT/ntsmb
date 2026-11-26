//! Stable media-identity selection helpers for UI adapters.
//!
//! A screen may persist the selected ID across recreation and restore it after a listing refresh
//! without depending on row positions or backend-specific entry types.

use crate::LibraryBrowser;

impl LibraryBrowser {
    /// Return the identity of the currently focused visible item.
    pub fn selected_media_id(&self) -> Option<&str> {
        self.selected_item().map(|item| item.id.as_str())
    }

    /// Select and activate a visible item by stable identity.
    ///
    /// A missing or filtered-out identity produces no intent and leaves the current focus intact.
    pub fn activate_media_id(&mut self, media_id: &str) -> crate::LibraryIntent {
        if !self.select_media_id(media_id) {
            return crate::LibraryIntent::None;
        }
        self.activate_selected()
    }

    /// Focus a visible item by its stable media identity.
    ///
    /// Returns true when the item is present in the current filtered listing. A missing or
    /// filtered-out ID leaves the current selection unchanged and returns false.
    pub fn select_media_id(&mut self, media_id: &str) -> bool {
        let Some(index) = self.visible_items().iter().position(|item| item.id == media_id) else {
            return false;
        };
        self.selected = index;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntsmb_core::{MediaFolder, MediaItem, MediaType};

    fn item(name: &str, path: &str) -> MediaItem {
        MediaItem { id: path.into(), name: name.into(), path: path.into(), size_bytes: None,
            is_directory: false, media_type: MediaType::Video }
    }

    fn folder(items: Vec<MediaItem>) -> MediaFolder {
        MediaFolder { source_id: "nas".into(), path: "Films".into(),
            parent_path: None, items }
    }

    #[test]
    fn selection_can_be_saved_and_restored_after_listing_reorder() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder(vec![item("A", "Films/A"), item("B", "Films/B")]));
        assert!(browser.select_media_id("Films/B"));
        let saved_id = browser.selected_media_id().unwrap().to_owned();

        browser.set_folder(folder(vec![item("B", "Films/B"), item("A", "Films/A")]));
        assert!(browser.select_media_id(&saved_id));
        assert_eq!(browser.selected_media_id(), Some("Films/B"));
    }

    #[test]
    fn activation_by_identity_selects_the_requested_media_without_row_assumptions() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder(vec![item("A", "Films/A"), item("B", "Films/B")]));
        assert_eq!(browser.activate_media_id("Films/B"), crate::LibraryIntent::PlayMedia {
            source_id: "nas".into(), path: "Films/B".into()
        });
        assert_eq!(browser.selected_media_id(), Some("Films/B"));
        assert_eq!(browser.activate_media_id("Films/missing"), crate::LibraryIntent::None);
        assert_eq!(browser.selected_media_id(), Some("Films/B"));
    }

    #[test]
    fn missing_or_filtered_item_does_not_change_current_selection() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(folder(vec![item("A", "Films/A"), item("B", "Films/B")]));
        assert!(browser.select_media_id("Films/B"));
        assert!(!browser.select_media_id("Films/missing"));
        assert_eq!(browser.selected_media_id(), Some("Films/B"));
        browser.set_query("A");
        assert!(!browser.select_media_id("Films/B"));
        assert_eq!(browser.selected_media_id(), Some("Films/A"));
    }
}
