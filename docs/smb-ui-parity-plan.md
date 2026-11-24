# SMB-first UI plan — Fin Native interaction styling, simple file browser

## Correct UI scope (explicit requirement)

This is **not** a Plex/Fin Native Home or Library UI clone. Do not keep or reproduce the Plex-style home, catalog, poster shelves, movie/TV detail pages, watchlists, server-driven navigation or metadata-first experience. The normal application should be a straightforward SMB file browser: connect to a share, browse folders, select a video and play it. Keep it simple and functional; no unnecessary artwork, hero banners, shelves, catalog pages or elaborate media discovery.

The requirement to retain Fin Native UI means retaining its **UI components and interaction polish**, especially:
- Fin Native / Apple-TV-like buttons, pills, panels and setting controls.
- The same directional focus/cursor movement, focus animation, focus highlight, and remote-control navigation behavior.
- Existing style, spacing, typography, colors, transitions and modal/panel presentation for settings and dialogs.
- Consistent OK/Select, Back, focus return/restoration, and scroll behavior on the TV remote.
- Reuse of its shared UI primitives, input/focus machinery, form/table/panel components and settings style.

Use these polished controls and focus behavior within a practical file-browser layout. Do not transplant the entire Fin Native content/navigation architecture.

## UI design direction

- **Main view:** simple folder/file list or compact grid; directory navigation and breadcrumbs/path display; supported video files identifiable by filename and optional small file-type icon.
- **Folder navigation:** enter a folder, go up/back, preserve sensible cursor position when returning; remote-first.
- **Settings:** Fin Native style settings screen and controls. Connection profiles, add/edit/delete, credentials, SMB options and playback preferences live here.
- **Connection form:** use the existing Fin Native form/input/button styling and focus behavior. Fields: server/host, share, optional starting path, username, password, guest mode; connect/test, validation and error states.
- **Dialogs:** use the existing Fin Native panel/modal and Apple-TV-like button presentation.
- **Playback:** retain the existing player and its controls/behavior if the existing player is reusable; do not make a Plex-style library UI prerequisite for playback.

## Existing code areas to reuse selectively

- Shared focus/input/remote navigation and UI primitives: `rust-modules/ui/src/`
- Settings screen and navigation patterns: `rust-modules/screens/src/settings.rs`
- Forms, panels, popovers, dialogs, controls and focus behavior in shared UI.
- Existing player UI/engine only where it can be used independently of Plex/server catalog flow.
- App routing and lifecycle: `rust-modules/src/app/`

**Do not preserve/recreate** Home, Plex catalog/library shelves, posters, detail screens, watchlists, server selection flows or metadata-oriented navigation just for visual parity. Only shared styling/interaction patterns are desired.

## Backend/product transformation

1. **SMB connection management** — Replace Plex sign-in/onboarding with SMB connection form. Host, share, username/password, guest access, optional starting path, test/connect and error feedback.
2. **Connection profiles** — Add/manage multiple SMB connections in settings. Store credentials securely using webOS Keymanager or an approved secure mechanism. Never plaintext credentials in ordinary settings/config, logs, URLs or repo.
3. **File browser** — Implement a plain remote-friendly folder/file browser backed by the SMB directory listing. Show folders and video files; browse, back, and open. Avoid unnecessary metadata/artwork.
4. **Playback** — Use the SMB range bridge and existing player handoff; preserve seeking via byte-range reads. Playback must work from direct file selection.
5. **Remove Plex dependency** — Remove Plex sign-in, server/catalog routes and unrelated server features once replacement paths work. Keep shared UI/input and reusable playback code.
6. **Optional metadata** — Not required for MVP; only consider later as optional enhancement.

## Acceptance criteria

- App's ordinary screen is a simple file browser, not a Plex-style media catalog.
- Settings, forms, buttons, panels, modal/dialog styling and remote cursor/focus behavior match Fin Native's polished interaction style.
- Remote directional navigation, focus highlight/movement, OK, BACK, focus restoration and scroll behavior are tested on TV.
- SMB connect, shares/folders and media file browsing work without Plex login or server.
- Selecting a file starts playback through the existing player with seeking support.
- Credentials securely stored and never leaked in logs or playback URLs.
- Debug build remains isolated from stable Fin Native.
- TV testing required before calling SMB UI/playback complete.

## Current status (2026-11-24)

- Repository fork and `smb-development` branch exist.
- SMB2/3 transport, directory listing, random-access reads, loopback range bridge and FFmpeg probe/player handoff foundation are present.
- Draft PR tracks backend implementation; current checked CI run was reported successful.
- SMB connection UI, simple file browser, secure credential persistence and Plex/server-flow removal are not implemented yet.
- This plan is a tracking document; it does not claim the end-user SMB app is finished.
