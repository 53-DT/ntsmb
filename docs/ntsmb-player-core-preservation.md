# NTSMB player-core preservation contract

This document is a migration guard for the FinNative/Jellyfin purge. The rooted-webOS native
playback repair is an NTSMB platform capability, not a FinNative feature. It must remain available
when inherited server, login, consent, locale and branding files are removed.

## Required behavior

1. At boot, the webOS platform identifies the affected `k5lp` / `k3lp` device family and checks
   whether `/dev/rtkmem` is readable from the app jail. The cached verdict is published through
   `plx_os::tv::native_video`; a blocked verdict prevents the native video Load.
2. Only the specific `jail_missing_rtkmem` failure offers **Repair**. Ordinary codec, source,
   timeout and pipeline failures must never invoke the jailer repair.
3. Repair requires a separate, explicit user confirmation. No boot hook or normal Play action may
   execute the privileged command.
4. The webOS implementation calls Homebrew Channel's elevated `exec` service, validates the app
   identity/install path before constructing the command, checks the exact success marker and then
   performs a fresh local readability check.
5. The player owns one repair attempt for the process lifetime. Screen recreation, playback changes,
   a failed worker and a timeout must not reset or duplicate the attempt. A timeout is an unknown
   remote outcome, not proof that the elevated command stopped.
6. Success requires a full app restart before another native playback attempt because the boot-time
   verdict is intentionally immutable for the running process.

## Neutral player-core boundary

The extracted player must not import `plx_os` or the webOS platform crate to make its repair
decision. The neutral contract now lives in `ntsmb_core`:

- `NativeVideoVerdict`: `Available`, `Blocked(reason)`, or `NotApplicable`.
- `NativeVideoRepairFailure`: stable failure categories, including timeout and unsupported.
- `NativeVideoRepairState`: process-lifetime `Idle` / `Running` / `Repaired` / `Failed`.
- `NativeVideoRepairAttempt`: the one-shot process-lifetime policy, including fail-closed unsupported-device handling and stale/duplicate receipt rejection. It has no reset API and is now implemented in `ntsmb_core`, not the legacy media machine.
- `NativeVideoRepairPort`: the player-facing `verdict()` and `repair()` boundary.

`plx_os::tv::native_video` re-exports those neutral types and provides
`InstalledRepairPort`, which delegates to the selected platform's installed repair hook.
The app composition injects that port into `PlayerAdapter`; the adapter spends the core-owned one-shot
authority and runs the repair off the frame thread. `plx_media::player::machine::RepairAttempt` remains a compatibility re-export so existing app/screen wiring does not change during extraction. The player core owns the decision and state,
not the privileged command or its platform identity checks.

Do not move the Homebrew Channel service call, jailer command, install-path validation, or
`/dev/rtkmem` probing into `ntsmb_core`, `library/`, or a generic UI module. Those remain in
the OS/platform boundary. This keeps NTSMB's player reusable without weakening the webOS repair.

## Code ownership — preserve these paths

- `rust-modules/os/src/tv/native_video.rs`: platform-neutral verdict, failure/state types and
  repair hook contract.
- `rust-modules/platform-webos/src/webos.rs`: affected-device probe and verdict publication.
- `rust-modules/platform-webos/src/webos/jail_repair.rs`: validated Homebrew Channel repair
  implementation and fail-closed verification.
- `rust-modules/platform-webos/src/port.rs`: registration of `jail_repair::execute` as the webOS
  repair hook. Its current install identity must remain unchanged until the user explicitly supplies
  the final bundle ID.
- `rust-modules/os/src/tv.rs`: optional platform repair hook.
- `rust-modules/core/src/lib.rs`: process-lifetime `NativeVideoRepairAttempt` policy and its fail-closed tests.
- `rust-modules/media/src/player/machine.rs`: compatibility re-export of the core repair authority.
- `rust-modules/media/src/player/adapter.rs`: off-frame repair worker and result polling.
- `rust-modules/screens/src/player/mod.rs`: failure action and explicit confirmation UI.
- `rust-modules/src/app/run.rs`: polling that remains active even when the player page is absent.

The generic NTSMB core must not import webOS, Homebrew Channel, FinNative, or Jellyfin. It may
expose neutral playback and lifecycle contracts, while the player coordinates the neutral contract
with the OS repair hook. The privileged implementation remains in the webOS platform crate.

## Purge acceptance checks

- No purge commit deletes or disables any path listed above.
- The `jail_missing_rtkmem` failure remains distinct from ordinary playback failures.
- The Repair action is still gated by the blocked verdict and idle one-shot repair state.
- The confirmation still defaults to Cancel.
- A successful repair still requires the exact marker plus fresh readability verification.
- The platform build still registers `jail_repair::execute` as the repair hook.
- Existing repair, jail-verdict, player-action and repair-receipt tests continue to pass.

Device-side repair execution must still be validated on an affected rooted TV; host build success alone
is not evidence that the Homebrew Channel command has run on hardware.
