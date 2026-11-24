# NT-SMB — file browser and native playback plan

## Non-negotiable architecture

NT-SMB is a lightweight SMB file browser for LG webOS. It is not a media-server client, media library, catalogue, scraper or home-theatre shell.

The three reference points have strictly separate jobs:

### Kodi: SMB connection behaviour only

Kodi is a behavioural reference because SMB browsing and authentication have already been tested successfully on the target television. Study Kodi only to understand and reproduce the SMB connection path:

- SMB2/SMB3 negotiation and compatibility behaviour.
- Server address, port, share connection and share listing.
- Username/password and guest authentication.
- Authentication edge cases and useful connection errors.
- Remembering connection details and reconnecting to a saved server.
- Directory listing and path handling over an authenticated SMB session.

Do not reuse Kodi's interface, navigation model, media-library concepts, artwork, metadata, playback code, player process, or application architecture. Do not add Kodi as a runtime dependency. When Kodi's exact behaviour is not established by source or an observed TV test, mark it unknown rather than guessing.

### FinNative: TV interaction, UI and playback

FinNative is the source of the user-facing TV experience and the playback implementation. Reuse its existing shared systems rather than recreating them:

- Remote key handling, focus engine, directional movement and focus restoration.
- Existing focus visuals, cursor behaviour, button and panel layouts.
- Shared animation, transitions, springs, scrolling and modal behaviour.
- Existing theme, typography, layout and reusable widgets.
- Existing player screen/HUD, playback session, backend contract, video-plane integration and native hardware playback pipeline.

Do not build a second player, introduce a new playback UI, or replace the existing native playback path. The SMB implementation supplies a seekable byte source to the existing player contract. Credentials and SMB protocol details must not appear in playback URLs or logs.

### NT-SMB: ordinary file manager

The NT-SMB application composes those capabilities into a straightforward file manager:

1. Connect to an SMB server/share (manual address first; discovery can follow).
2. Authenticate, including guest access where supported.
3. Browse folders and video files in a plain, readable list.
4. Move through folders, go to the parent, and retain focus/cursor position sensibly.
5. Select a video and hand it to FinNative's existing player/native playback pipeline.

The browser should be deliberately ordinary: clear file/folder names, folder-first sorting, useful path/breadcrumb, concise connection status and errors. No posters, hero banners, metadata, scraping, watchlists, libraries, recommendations, Plex, Jellyfin, or Kodi UI.

## MVP screens

1. **Connect** — minimal host, share, username, password and guest controls. This small form is allowed immediately because it is needed to prove the SMB-to-player path; it does not need to wait for a finished browser UI.
2. **Browse** — plain folder/video list using FinNative's shared focus, remote navigation, layout and motion systems.
3. **Playback** — selected file enters FinNative's existing player and native hardware pipeline through the SMB range bridge.
4. **Saved connections** — multiple profiles and secure credential storage only after the basic connection/playback path is proven.

## Implementation order

1. Keep the SMB transport isolated and prove an authenticated connection and directory listing on the TV.
2. Prove that one SMB video can be opened through the range bridge and played by FinNative's existing player.
3. Test actual TV playback, audio, seeking, stop/return and reconnect behaviour.
4. Build the ordinary browser screen using existing FinNative shared widgets and focus/navigation systems.
5. Add saved profiles and secure credential persistence.
6. Add optional network discovery only after manual-IP connections work.
7. Remove inherited Plex/Jellyfin-only routes and dependencies after the SMB path is protected by tests. Do not delete shared UI, playback, platform or codec code.

## Acceptance criteria

- Kodi is used only as an SMB behaviour reference; none of its UI or player enters NT-SMB.
- Browser interaction and animation use FinNative's existing shared UI systems.
- Video playback uses FinNative's existing player and native webOS hardware path.
- SMB credentials never appear in URLs, logs, commits or ordinary plaintext settings.
- TV remote focus, movement, selection, back navigation and focus restoration work.
- A real SMB video plays on the target TV, seeks, and stops cleanly.
- The debug install remains isolated from the user's stable FinNative install.

## Current status

- SMB2/3 backend, share-relative directory listing, random-access file reads and loopback HTTP Range bridge are present in the development branch.
- Pure browser helpers exist for folders-first sorting, video extension filtering, safe child paths, breadcrumbs and parent navigation.
- The actual SMB connection/browser screen is not yet wired into app routing.
- End-to-end SMB-to-FinNative-player playback is not yet proven on the TV.
- Saved profile persistence, credential protection and Plex/Jellyfin flow removal remain outstanding.
- The latest CI run is red; its failures must be fixed before an IPK is considered ready for TV testing.
