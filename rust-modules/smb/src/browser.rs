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


/// The file browser's logical presentation state. This is deliberately independent of network IO:
/// the app worker supplies a directory listing, while the screen owns path, cursor and navigation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowserModel {
    /// Share-relative directory; empty means the share root.
    path: String,
    /// Visible rows after filtering and sorting.
    entries: Vec<BrowserEntry>,
    /// Focused row index, clamped whenever a listing changes.
    selected: usize,
    /// Last selected child directory, used when returning from a nested folder.
    parent_selection: Option<(String, usize)>,
    /// Listing error shown in the browser read-out.
    error: Option<String>,
    loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserAction {
    OpenDirectory { path: String, restore_selection: usize },
    PlayFile { path: String },
    None,
}

impl BrowserModel {
    pub fn path(&self) -> &str { &self.path }
    pub fn entries(&self) -> &[BrowserEntry] { &self.entries }
    pub fn selected(&self) -> usize { self.selected }
    pub fn error(&self) -> Option<&str> { self.error.as_deref() }
    pub fn is_loading(&self) -> bool { self.loading }
    pub fn crumbs(&self) -> Vec<String> { breadcrumbs(&self.path) }

    /// Begin a directory request. The current row is retained so BACK can restore it.
    pub fn begin_open(&mut self, path: String, restore_selection: usize) {
        self.loading = true;
        self.error = None;
        self.parent_selection = Some((path.clone(), restore_selection));
        self.path = path;
        self.entries.clear();
        self.selected = 0;
    }

    /// Publish a successful worker result and seat focus at the requested row.
    pub fn publish(&mut self, rows: impl IntoIterator<Item = Entry>, selection: usize) {
        self.entries = visible_entries(rows);
        self.selected = selection.min(self.entries.len().saturating_sub(1));
        self.loading = false;
        self.error = None;
    }

    /// Publish a failed directory request without losing the current path.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.loading = false;
        self.error = Some(message.into());
        self.entries.clear();
        self.selected = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() { self.selected = 0; return; }
        let last = self.entries.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// Resolve OK on the focused row. The caller performs network navigation or native playback.
    pub fn activate(&self) -> BrowserAction {
        let Some(row) = self.entries.get(self.selected) else { return BrowserAction::None; };
        match child_path(&self.path, &row.entry.name) {
            Ok(path) if row.entry.is_directory => BrowserAction::OpenDirectory {
                path,
                restore_selection: self.selected,
            },
            Ok(path) if row.is_video => BrowserAction::PlayFile { path },
            _ => BrowserAction::None,
        }
    }

    /// Resolve BACK. Returns the parent path and the child row to restore focus to.
    pub fn back(&self) -> Option<(String, usize)> {
        let parent = parent_path(&self.path)?;
        let selection = self.parent_selection
            .as_ref()
            .filter(|(opened, _)| opened == &self.path)
            .map(|(_, selection)| *selection)
            .unwrap_or(0);
        Some((parent, selection))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
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
    fn child_path_preserves_unicode_and_nonbreaking_spaces_exactly() {
        let name = "My Hero Academia S01E01\u{00a0}- “Izuku Midoriya-Origin”.mkv";
        assert_eq!(child_path("Season 1", name).unwrap(), format!("Season 1/{name}"));
    }

    #[test]
    fn breadcrumbs_and_parent_path_are_share_relative() {
        assert_eq!(breadcrumbs("Movies/Drama/"), ["Movies", "Drama"]);
        assert_eq!(breadcrumbs(""), Vec::<String>::new());
        assert_eq!(parent_path("Movies/Drama"), Some("Movies".into()));
        assert_eq!(parent_path("Movies"), Some(String::new()));
        assert_eq!(parent_path(""), None);
    }

    #[test]
    fn browser_model_keeps_cursor_and_returns_folder_and_file_actions() {
        let mut model = BrowserModel::default();
        model.publish([
            row("Film.mkv", false), row("A folder", true), row("poster.jpg", false),
        ], 1);
        assert_eq!(model.selected(), 1);
        assert_eq!(model.activate(), BrowserAction::OpenDirectory {
            path: "A folder".into(), restore_selection: 1,
        });
        model.begin_open("A folder".into(), 1);
        model.publish([row("Episode.mp4", false)], 0);
        assert_eq!(model.activate(), BrowserAction::PlayFile { path: "A folder/Episode.mp4".into() });
        assert_eq!(model.back(), Some((String::new(), 1)));
    }

    #[test]
    fn browser_model_clamps_selection_and_exposes_errors() {
        let mut model = BrowserModel::default();
        model.publish([row("a.mkv", false), row("b.mkv", false)], 99);
        assert_eq!(model.selected(), 1);
        model.fail("Share disconnected");
        assert_eq!(model.error(), Some("Share disconnected"));
        assert!(!model.is_loading());
        assert!(model.entries().is_empty());
    }

}


/// Build a stable namespace for one SMB share without including credentials.
///
/// The connection key identifies host, port and account; the share component prevents
/// equal relative paths in different shares from colliding. Length-prefixing keeps the
/// encoding unambiguous even when names contain punctuation.
pub fn media_source_id(profile: &crate::profile::ConnectionProfile, share: &str) -> String {
    let connection = crate::store::connection_id(profile);
    format!("{}:{}{}:{}", connection.len(), connection, share.len(), share)
}

/// Build the stable identity used by the neutral library for an SMB item.
///
/// The source namespace prevents equal relative paths on different saved connections from
/// sharing progress or selection identity. Keep this helper as the single encoding rule used by
/// both folder conversion and screen-side focus restoration.
pub fn media_id(source_id: &str, item_path: &str) -> String {
    format!("{source_id}:{item_path}")
}

/// Resolve a visible SMB entry to the same stable identity used by the neutral library.
/// Keeping this conversion here prevents screens from reimplementing SMB path rules.
pub fn entry_media_id(source_id: &str, folder: &str, entry_name: &str) -> Result<String, String> {
    let path = child_path(folder, entry_name)?;
    Ok(media_id(source_id, &path))
}

/// Find the SMB row corresponding to a neutral library media identity.
pub fn entry_index_for_media_id(
    source_id: &str,
    folder: &str,
    entries: &[BrowserEntry],
    target_media_id: &str,
) -> Option<usize> {
    entries.iter().position(|row| {
        entry_media_id(source_id, folder, &row.entry.name)
            .map(|id| id == target_media_id)
            .unwrap_or(false)
    })
}

/// Translate an SMB directory result into the backend-neutral library contract.
/// Path and source identity are supplied by the caller; no SMB credential or protocol
/// detail is exposed through the returned model.
pub fn to_core_folder(
    source_id: &str,
    path: &str,
    entries: impl IntoIterator<Item = Entry>,
) -> Result<ntsmb_core::MediaFolder, String> {
    let mut items = Vec::new();
    for entry in entries {
        let item_path = child_path(path, &entry.name)?;
        let media_type = if entry.is_directory {
            ntsmb_core::MediaType::Folder
        } else {
            media_type_for_name(&entry.name)
        };
        items.push(ntsmb_core::MediaItem {
            id: media_id(source_id, &item_path),
            name: entry.name,
            path: item_path,
            size_bytes: (!entry.is_directory).then_some(entry.size),
            is_directory: entry.is_directory,
            media_type,
        });
    }
    Ok(ntsmb_core::MediaFolder {
        source_id: source_id.to_owned(),
        path: path.trim_matches('/').to_owned(),
        parent_path: parent_path(path),
        items,
    })
}

fn media_type_for_name(name: &str) -> ntsmb_core::MediaType {
    let extension = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    if is_video_name(name) {
        ntsmb_core::MediaType::Video
    } else if ["aac", "ac3", "aiff", "alac", "flac", "m4a", "mp3", "ogg", "opus", "wav", "wma"].iter().any(|known| extension.eq_ignore_ascii_case(known)) {
        ntsmb_core::MediaType::Audio
    } else if ["bmp", "gif", "heic", "jpeg", "jpg", "png", "tif", "tiff", "webp"].iter().any(|known| extension.eq_ignore_ascii_case(known)) {
        ntsmb_core::MediaType::Image
    } else {
        ntsmb_core::MediaType::Other
    }
}


#[cfg(test)]
mod core_contract_tests {
    use super::*;

    #[test]
    fn media_source_identity_separates_connections_and_shares() {
        let profile_a = crate::profile::ConnectionProfile::new("nas-a", 445, "alice", false).unwrap();
        let profile_b = crate::profile::ConnectionProfile::new("nas-b", 445, "alice", false).unwrap();
        let other_user = crate::profile::ConnectionProfile::new("nas-a", 445, "bob", false).unwrap();

        let movies = media_source_id(&profile_a, "Movies");
        assert_eq!(movies, media_source_id(&profile_a, "Movies"));
        assert_ne!(movies, media_source_id(&profile_a, "TV"));
        assert_ne!(movies, media_source_id(&profile_b, "Movies"));
        assert_ne!(movies, media_source_id(&other_user, "Movies"));
    }

    #[test]
    fn media_identity_is_stable_and_namespaced_by_source() {
        assert_eq!(media_id("server-a", "Films/Arrival.mkv"), "server-a:Films/Arrival.mkv");
        assert_eq!(media_id("server-a", "Films/Arrival.mkv"),
            media_id("server-a", "Films/Arrival.mkv"));
        assert_ne!(media_id("server-a", "Films/Arrival.mkv"),
            media_id("server-b", "Films/Arrival.mkv"));
        assert_ne!(media_id("server-a", "Films/Arrival.mkv"),
            media_id("server-a", "Films/Arrival (2).mkv"));
    }

    #[test]
    fn smb_listing_maps_to_backend_neutral_folder_contract() {
        let folder = to_core_folder(
            "nas-movies",
            "Films/Drama",
            vec![
                Entry { name: "Arrival.mkv".into(), size: 1024, is_directory: false },
                Entry { name: "Extras".into(), size: 0, is_directory: true },
                Entry { name: "notes.txt".into(), size: 20, is_directory: false },
            ],
        ).unwrap();

        assert_eq!(folder.source_id, "nas-movies");
        assert_eq!(folder.path, "Films/Drama");
        assert_eq!(folder.parent_path.as_deref(), Some("Films"));
        assert_eq!(folder.items.len(), 3);
        assert_eq!(folder.items[0].media_type, ntsmb_core::MediaType::Video);
        assert_eq!(folder.items[0].path, "Films/Drama/Arrival.mkv");
        assert_eq!(folder.items[1].media_type, ntsmb_core::MediaType::Folder);
        assert_eq!(folder.items[2].media_type, ntsmb_core::MediaType::Other);
    }

    #[test]
    fn root_listing_maps_without_a_parent() {
        let folder = to_core_folder(
            "nas-movies",
            "",
            vec![Entry { name: "Movie.mp4".into(), size: 7, is_directory: false }],
        ).unwrap();
        assert_eq!(folder.path, "");
        assert_eq!(folder.parent_path, None);
        assert_eq!(folder.items[0].path, "Movie.mp4");
    }
    #[test]
    fn core_mapping_classifies_audio_and_images_and_rejects_invalid_names() {
        let folder = to_core_folder("nas-media", "", vec![
            Entry { name: "theme.FLAC".into(), size: 9, is_directory: false },
            Entry { name: "poster.webp".into(), size: 8, is_directory: false },
        ]).unwrap();
        assert_eq!(folder.items[0].media_type, ntsmb_core::MediaType::Audio);
        assert_eq!(folder.items[1].media_type, ntsmb_core::MediaType::Image);
        assert!(to_core_folder("nas-media", "", vec![
            Entry { name: "../escape.mkv".into(), size: 1, is_directory: false },
        ]).is_err());
    }
    #[test]
    fn entry_identity_round_trips_to_the_visible_smb_row() {
        let entries = visible_entries([
            Entry { name: "A folder".into(), size: 0, is_directory: true },
            Entry { name: "Arrival.mkv".into(), size: 1, is_directory: false },
        ]);
        let id = entry_media_id("source-a", "Films", "Arrival.mkv").unwrap();
        assert_eq!(entry_index_for_media_id("source-a", "Films", &entries, &id), Some(1));
        assert_eq!(entry_index_for_media_id("source-b", "Films", &entries, &id), None);
        assert_eq!(entry_index_for_media_id("source-a", "Other", &entries, &id), None);
        assert!(entry_media_id("source-a", "Films", "../escape.mkv").is_err());
    }

}
