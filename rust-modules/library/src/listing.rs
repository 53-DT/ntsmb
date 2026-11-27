//! Pure search, filtering and ordering for neutral media-folder listings.

use ntsmb_core::{MediaFolder, MediaItem, MediaType};

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
            Self::Video => !item.is_directory && item.media_type == MediaType::Video,
            Self::Audio => !item.is_directory && item.media_type == MediaType::Audio,
            Self::Images => !item.is_directory && item.media_type == MediaType::Image,
        }
    }
}

/// Derive the visible rows without mutating the source folder.
pub(crate) fn visible_items<'a>(
    folder: &'a MediaFolder,
    query: &str,
    filter: MediaFilter,
    sort: LibrarySort,
) -> Vec<&'a MediaItem> {
    let needle = query.trim().to_lowercase();
    let mut rows: Vec<&MediaItem> = folder.items.iter()
        .filter(|item| filter.matches(item))
        .filter(|item| needle.is_empty() || item.name.to_lowercase().contains(&needle))
        .collect();
    rows.sort_by(|a, b| {
        let folders_first = b.is_directory.cmp(&a.is_directory);
        match sort {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ntsmb_core::{MediaItem, MediaType};

    fn item(name: &str, directory: bool, media_type: MediaType) -> MediaItem {
        MediaItem {
            id: name.into(), name: name.into(), path: name.into(),
            size_bytes: None, is_directory: directory, media_type,
        }
    }

    fn folder(items: Vec<MediaItem>) -> MediaFolder {
        MediaFolder { source_id: "smb".into(), path: String::new(),
            parent_path: None, items }
    }

    #[test]
    fn filters_and_sorts_are_applied_without_mutating_the_source_listing() {
        let source = folder(vec![
            item("zeta.mkv", false, MediaType::Video),
            item("Extras", true, MediaType::Folder),
            item("Alpha.mp4", false, MediaType::Video),
            item("cover.jpg", false, MediaType::Image),
        ]);
        let rows = visible_items(&source, "ALPHA", MediaFilter::All, LibrarySort::Name);
        assert_eq!(rows.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["Alpha.mp4"]);
        assert_eq!(source.items.len(), 4);
        let rows = visible_items(&source, "", MediaFilter::Video, LibrarySort::NameDescending);
        assert_eq!(rows.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["zeta.mkv", "Alpha.mp4"]);
    }

    #[test]
    fn media_only_filters_exclude_directories_even_when_their_type_is_misclassified() {
        let source = folder(vec![
            item("video-folder", true, MediaType::Video),
            item("video.mkv", false, MediaType::Video),
            item("audio-folder", true, MediaType::Audio),
        ]);
        let rows = visible_items(&source, "", MediaFilter::Video, LibrarySort::Name);
        assert_eq!(rows.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["video.mkv"]);
    }
}
