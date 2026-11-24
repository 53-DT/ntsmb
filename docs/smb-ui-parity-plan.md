# SMB-first UI migration plan (Fin Native parity)

## Non-negotiable UI rule

The SMB edition must preserve the Fin Native navigation model and visual language. This is a backend/source migration, not a redesign. Reuse existing screen components, layout, focus/input handling, transitions, remote-key behavior, poster/card components, typography, spacing, and player UI wherever possible. New SMB views must be built from the same UI primitives and match the original look.

## Existing implementation to preserve

- Home: `rust-modules/screens/src/home/`
- Login/onboarding: `rust-modules/screens/src/login.rs` and related login flow
- Library: `rust-modules/screens/src/library/`
- Settings: `rust-modules/screens/src/settings.rs`
- Detail and player screens: `rust-modules/screens/src/detail/`, `rust-modules/screens/src/player/`
- App routing and boot: `rust-modules/src/app/`
- Shared UI primitives: `rust-modules/ui/src/`

Do not replace these screens with a web view, HTML interface, or a separate UI framework.

## Product transformation

1. **SMB connection management** — Replace Plex sign-in/onboarding with an SMB connection screen using the existing screen/panel/form primitives and remote focus behavior. Support host, share, username and password; guest access; connection test; validation and useful error states.
2. **Connection list** — Save multiple SMB server/share profiles. Credential storage must use webOS Keymanager or an approved secure mechanism; do not store plaintext passwords in normal settings/config files, logs, URLs or repository artifacts.
3. **Browse shares and folders** — Implement a remote-friendly directory browser with the existing navigation, focus, scroll and visual patterns. Directory rows and media items should match Fin Native styling. Preserve BACK semantics and focus restoration.
4. **Media selection/playback** — Filter supported media files, prepare the SMB-backed range stream, and hand off to the existing player. Preserve the player screen, controls, track selection and playback UX.
5. **Remove server dependency** — Remove Plex authentication, server selection and server-specific routes only after SMB replacement flows work. Keep shared native player and UI infrastructure intact.
6. **Optional metadata** — Metadata/artwork is a later enhancement and must not be required to connect, browse, or play files.

## Acceptance criteria

- Visually and behaviorally consistent with Fin Native; no redesign of navigation.
- Works with TV remote: directional navigation, OK, BACK, focus restoration and keyboard/remote focus are tested.
- SMB browsing and connection work without Plex login or a media server.
- Playback uses the existing native player UI and supports seeking through SMB range reads.
- Credentials never appear in logs or playback URLs and are stored securely.
- Debug build is isolated from the stable Fin Native install.
- TV testing is required before calling the SMB path complete.

## Current status (2026-11-24)

- Repository fork and `smb-development` branch exist.
- SMB2/3 transport, directory listing, random-access reads, loopback range bridge and FFmpeg probe/player handoff foundation are present.
- A draft PR tracks this backend work; current workflow run reported success for the checked build.
- SMB connection UI, browser UI, secure credential persistence and removal of Plex/server-dependent screens are not implemented yet.
- This plan is a tracking document; it does not claim that the browser or end-user SMB experience is complete.
