# SMB file-browser plan — simple layout, polished TV controls

## UI scope

The application is a **simple SMB file browser and native video player**, not a Plex/Jellyfin media catalog.

Use Fin Native's UI primitives and interaction polish selectively:
- rounded Apple-TV-like buttons, settings controls and panels
- focus/cursor movement, focus highlighting and transitions
- remote-control directional navigation, OK/Select, BACK and focus restoration
- established spacing, typography, colors and modal/dialog style for settings and forms

Do **not** retain or recreate Home, libraries, poster shelves, hero banners, movie/TV detail pages, watchlists, metadata-driven navigation or catalog/discovery screens.

## MVP screens

1. **Connections/settings** — add/edit/remove SMB profiles; host, share, optional path, username/password, guest mode, test/connect; use the existing settings/form/control style.
2. **File browser** — plain folder/file list or compact grid, current path/breadcrumb, folders and supported video filenames. Open folders, go up, preserve cursor where practical. Keep it simple; no artwork or metadata requirement.
3. **Playback** — selecting a video starts it using the SMB range bridge and reusable native player, with seeking if supported.
4. **Connection profiles** — multiple profiles; credentials must use secure storage (webOS Keymanager or approved mechanism), never plaintext in ordinary settings, logs or URLs.
5. **Remove Plex/Jellyfin-specific flow** — remove sign-in, server/catalog routes and irrelevant functionality once SMB flow works. Keep reusable controls/input and playback infrastructure.
6. **Optional enhancements later** — metadata or thumbnails are not MVP and must not be prerequisites for browsing or playback.

## Acceptance criteria

- Main UI is a practical file browser.
- Settings, buttons, panels, dialogs and focus/remote behavior retain the desired Fin Native polish.
- TV remote directional movement, focus, select, back and focus restoration tested on target TV.
- Browse SMB shares/folders without Plex or Jellyfin server.
- Play selected video through existing native player; seeking uses SMB range reads.
- Credentials protected and not leaked in logs or URLs.
- Debug app install is isolated from stable app.
- TV testing required before declaring MVP complete.

## Status

- Fork and development branch exist.
- SMB2/3 backend, directory listing, random-access reads, loopback range bridge and player handoff foundation are present.
- Added pure file-browser helpers for folder-first sorting, video extension filtering, child-path safety, breadcrumbs and parent navigation, with unit tests.
- The actual remote-control browser screen and SMB connection UI are still not wired into app routing.
- Secure credential persistence, removal of media-server flows and end-to-end TV testing remain outstanding.
