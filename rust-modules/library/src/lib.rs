//! Backend-neutral library module exports for NTSMB.

mod browser;
mod listing;
pub use browser::{LibraryBrowser, LibraryIntent};
pub use listing::{LibrarySort, MediaFilter};

#[cfg(test)]
use ntsmb_core::{MediaFolder, MediaItem, MediaType};

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


#[cfg(test)]
mod refresh_removed_selection_tests {
    use super::*;

    fn media(id: &str, name: &str, media_type: MediaType) -> MediaItem {
        MediaItem {
            id: id.into(),
            name: name.into(),
            path: name.into(),
            size_bytes: None,
            is_directory: false,
            media_type,
        }
    }

    #[test]
    fn refreshing_when_focused_item_disappears_clamps_to_a_valid_row() {
        let mut browser = LibraryBrowser::default();
        browser.set_folder(MediaFolder {
            source_id: "smb".into(),
            path: "Films".into(),
            parent_path: None,
            items: vec![
                media("a", "Alpha.mkv", MediaType::Video),
                media("b", "Bravo.mkv", MediaType::Video),
                media("c", "Charlie.mkv", MediaType::Video),
            ],
        });
        browser.select_media_id("c");
        assert_eq!(browser.selected_item().map(|item| item.id.as_str()), Some("c"));

        browser.set_folder(MediaFolder {
            source_id: "smb".into(),
            path: "Films".into(),
            parent_path: None,
            items: vec![
                media("a", "Alpha.mkv", MediaType::Video),
                media("b", "Bravo.mkv", MediaType::Video),
            ],
        });

        assert_eq!(browser.selected(), 1);
        assert_eq!(browser.selected_item().map(|item| item.id.as_str()), Some("b"));
    }
}
