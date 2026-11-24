//! Pure SMB file-browser presentation helpers.
//!
//! This module keeps the normal browser intentionally plain: folders first, then video files,
//! alphabetically, with no server metadata or artwork dependency. The UI can consume these helpers
//! without putting path policy or extension filtering into a render frame.

use crate::Entry;

/// A visible item in the SMB browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEntry {
    pub entry: Entry,
    /// True for a video file recognized by the extension allow-list.
    pub is_video: bool,
}

/// Video types supported by the file browser's MVP filter.
/// Playback support still depends on the TV decoder and the existing player.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "avi", "m2ts", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts", "mxf", "ts", "webm", "wmv",
];

/// Filter out non-video files and sort folders first, then visible files by case-insensitive name.
/// Folder rows are always retained regardless of their extension.
pub fn visible_entries(entries: impl IntoIterator<Item = Entry>) -> Vec<BrowserEntry> {
    let mut visible: Vec<_> = entries
        .into_iter()
        .filter_map(|entry| {
            if entry.is_directory {
                Some(BrowserEntry { entry, is_video: false })
            } else if is_video_name(&entry.name) {
                Some(BrowserEntry { entry, is_video: true })
            } else {
                None
            }
        })
        .collect();

    visible.sort_by(|a, b| {
        b.entry.is_directory
            .cmp(&a.entry.is_directory)
            .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
            .then_with(|| a.entry.name.cmp(&b.entry.name))
    });
    visible
}

/// Whether a filename's extension is one of the browser's video types.
pub fn is_video_name(name: &str) -> bool {
    name.rsplit_once('.')
        .map(|(_, extension)| VIDEO_EXTENSIONS.iter().any(|known| extension.eq_ignore_ascii_case(known)))
        .unwrap_or(false)
}

/// Join a current share-relative directory and one directory/file name.
/// Rejects names that could escape or introduce another path segment.
pub fn child_path(current: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err("Invalid SMB entry name".into());
    }
    let current = current.trim_matches('/');
    if current.is_empty() {
        Ok(name.to_owned())
    } else {
        Ok(format!("{current}/{name}"))
    }
}

/// Split a share-relative path into breadcrumb labels. Root is represented by an empty vector.
pub fn breadcrumbs(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(str::to_owned)
        .collect()
}

/// Parent of a share-relative directory. Root has no parent.
pub fn parent_path(path: &str) -> Option<String> {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() {
        return None;
    }
    normalized.rsplit_once('/').map(|(parent, _)| parent.to_owned()).or(Some(String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, is_directory: bool) -> Entry {
        Entry { name: name.into(), size: 0, is_directory }
    }

    #[test]
    fn folders_first_then_video_files_alphabetically() {
        let actual = visible_entries([
            row("zeta.mp4", false),
            row("Docs", true),
            row("alpha.mkv", false),
            row("music.mp3", false),
            row("Movies", true),
            row("hidden.txt", false),
        ]);
        assert_eq!(actual.iter().map(|item| item.entry.name.as_str()).collect::<Vec<_>>(),
            ["Docs", "Movies", "alpha.mkv", "zeta.mp4"]);
    }

    #[test]
    fn extension_filter_is_case_insensitive_and_handles_no_extension() {
        assert!(is_video_name("episode.MKV"));
        assert!(is_video_name("movie.m2ts"));
        assert!(!is_video_name("cover.jpg"));
        assert!(!is_video_name("README"));
    }

    #[test]
    fn child_path_rejects_traversal_and_path_separators() {
        assert_eq!(child_path("Movies", "Film.mkv").unwrap(), "Movies/Film.mkv");
        assert_eq!(child_path("", "Films").unwrap(), "Films");
        assert!(child_path("Movies", "..").is_err());
        assert!(child_path("Movies", "Other/Film.mkv").is_err());
        assert!(child_path("Movies", "Other\\Film.mkv").is_err());
    }

    #[test]
    fn breadcrumbs_and_parent_path_are_share_relative() {
        assert_eq!(breadcrumbs("Movies/Drama/"), ["Movies", "Drama"]);
        assert_eq!(breadcrumbs(""), Vec::<String>::new());
        assert_eq!(parent_path("Movies/Drama"), Some("Movies".into()));
        assert_eq!(parent_path("Movies"), Some(String::new()));
        assert_eq!(parent_path(""), None);
    }
}
