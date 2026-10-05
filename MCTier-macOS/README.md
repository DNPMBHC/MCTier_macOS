# MCTier macOS Preview

macOS adaptation contribution: [DNPMBHC/MCTier_macOS](https://github.com/DNPMBHC/MCTier_macOS).

The original MCTier author information, copyright, license boundaries, and
EasyTier third-party notices remain authoritative in the repository root.

This directory contains the macOS build and native-resource preparation scripts.

## Status

- Tauri `.app`/`.dmg` packaging is configured for macOS 11+.
- Apple Silicon (`aarch64-apple-darwin`) is the primary target; Intel uses `x86_64-apple-darwin`.
- macOS networking requires EasyTier `easytier-core` and `easytier-cli` built as matching Mach-O executables.
- Creating the `utun` adapter requires root, so `easytier-core` is launched through the system administrator authorization prompt (see below). The MCTier UI itself stays unprivileged.
- The repository intentionally does not fall back to Linux ELF binaries and does not claim virtual-LAN support until those resources are verified.
- The macOS main window uses native decorations and the standard left-side red/yellow/green traffic lights. It opens in the compact portrait layout (420×680) and switches to the desktop two-column layout once the window is dragged to 760px or wider; dragging back below that restores portrait. The breakpoint lives in `src/utils/windowLayout.ts` and is mirrored by `@media (min-width: 760px)` in the macOS CSS, so the JS form and the stylesheet always agree. The green button zooms or enters native full screen.
- Hiding the window (the in-app minimize button, the summon hotkey, or the tray menu) keeps MCTier running in the menu bar. Clicking the menu bar icon, the Dock icon, or reopening the app all restore it — macOS reports the latter two as a `Reopen` event, which the Rust side handles explicitly. Without that handler the window could be hidden or minimized with no way to bring it back.
- The red close button hides to the menu bar by default on macOS (`close_to_tray` defaults to true there, false elsewhere). Closing used to quit, which on a Mac reads as "the window vanished and clicking the icon does nothing" — the app was gone, so the icon only cold-started it. The default lives in `config_manager::default_close_to_tray()` and is shared by the close handler, the settings command, and the settings page, so the toggle never contradicts the actual behavior. An explicitly saved `false` still wins; turn it off in Settings → 关闭时最小化到托盘.
- Secondary overlays (screen viewer, danmaku, and game HUD) retain their dedicated transparent/overlay behavior.
- Microphone capture has a native CoreAudio path, matching the Windows WASAPI one (see below). Native screen capture has a CoreGraphics path (`src-tauri/src/modules/native_capture/macos.rs`) matching the Windows Graphics Capture one: display and window sources, requested-resolution scaling, and the same binary frame packets. macOS requires the user to grant Screen Recording permission first (see below). Recording system audio has a ScreenCaptureKit path on macOS 13+ (see below); older systems need a virtual device such as BlackHole.
- The traffic-light, Cmd+W/Cmd+Q, multi-monitor restore, Retina sizing, full-screen, portrait/landscape layout switching, microphone capture, and native screen capture behavior require on-device validation.

## Prepare EasyTier

The default preparation path builds EasyTier from the pinned upstream source commit. It does not guess a release URL or reuse a Linux ELF binary:

```bash
./MCTier-macOS/scripts/build-easytier.sh
./MCTier-macOS/scripts/fetch-binaries.sh
```

The source build is pinned to EasyTier `v2.5.0` commit
`88a45d115670631dfe6a05ba192387d615ddb95b`. The script verifies the checked-out
commit, builds `easytier-core` and `easytier-cli` for the host architecture,
checks their Mach-O type and `--version` output, prints SHA-256 values, and only
then installs them atomically under:

```text
src-tauri/resources/binaries/macos/aarch64-apple-darwin/
src-tauri/resources/binaries/macos/x86_64-apple-darwin/
```

To build from an already reviewed local checkout, use
`--source /path/to/EasyTier` or set `MCTIER_EASYTIER_SOURCE_DIR`. The checkout
must be exactly the pinned commit. `fetch-binaries.sh` then performs the final
Mach-O and executable-permission check. The macOS build script automatically
runs the pinned source build when these files are absent.

The corresponding EasyTier source is LGPL-3.0. Record the printed commit and
SHA-256 values with any release artifact before distributing it.

## Administrator authorization when creating or joining a lobby

macOS only lets a root process create a `utun` interface, so an unprivileged
`easytier-core` fails immediately with
`rust tun error Operation not permitted (os error 1)`. MCTier therefore does not
spawn `easytier-core` directly: on every lobby start it asks macOS for
administrator authorization with
`osascript -e 'do shell script "..." with administrator privileges'` and runs a
small supervisor script as root. The supervisor

- forwards `easytier-core` stdout/stderr through FIFOs so the normal log
  parsing and virtual-IP detection are unchanged;
- terminates `easytier-core` when the app writes a stop sentinel **or** when
  the MCTier process itself disappears, so quitting or crashing the app never
  leaves a root-owned orphan process or a stuck `utun` interface.

The authorization dialog asks for the current user's login password. Each lobby
start spawns a fresh `osascript` process with a different script body, and macOS
only reuses an authorization cache for the same compiled script in the same
process, so **expect one prompt per lobby start**. The frontend warns about the
prompt in the "connecting" toast; cancelling the dialog fails the lobby with a
dedicated message instead of silently retrying.

Only `easytier-core` runs as root. The MCTier UI, the WebView, and all chat and
voice code stay unprivileged, and no privileged helper is installed
permanently. Delivering a signed SMAppService / SMJobBless helper is the way to
reduce this to a single authorization per install, and is deliberately left for a
signed release.

## Microphone and audio

Microphone capture has a native macOS path in
`src-tauri/src/modules/native_microphone/macos.rs`, matching what WASAPI does on
Windows. It is a CoreAudio HAL output unit in input-only mode, driven through the
C AudioUnit and AudioObject APIs (declared locally, so no extra crate is pulled in).

- Devices come from the CoreAudio object API and are identified by their stable
  UID (`coreaudio:<uid>`), so replugging a device does not invalidate a saved
  selection. Only endpoints with input streams are listed.
- Audio is delivered in exactly the same packets as Windows: 48 kHz mono float32,
  960 samples (3840 bytes) per 20 ms packet. The frontend AudioWorklet pump and its
  packet validation are unchanged. A device that refuses the 48 kHz client format
  is resampled to it rather than played back at the wrong speed.
- `system_processing` selects the system voice-processing unit, which is where
  macOS applies echo cancellation and noise suppression. If that unit cannot be
  opened, capture falls back to the plain HAL unit with a log warning.

Unlike Windows, macOS keeps the browser capture path as a safety net: when the
native unit cannot be opened the frontend falls back to `getUserMedia` rather than
losing the microphone entirely. Windows deliberately does not, because bypassing
the WebView2 microphone chain is the reason the native path exists there.

Recording what the machine **plays** is supported on macOS 13+ through
ScreenCaptureKit (`src-tauri/src/modules/native_microphone/macos_system_audio.rs`),
matching the Windows WASAPI loopback: 48 kHz mono float32 packets, the same
batching and two-second backlog stop, and current-process audio excluded. It
requires Screen Recording permission — the same grant the recorder's video path
needs. On macOS 12 and older the path reports the BlackHole virtual-device
workaround instead of capturing.

While voice or capture sessions are active the app claims
`NSActivityUserInitiatedAllowingIdleSystemSleep`, so hiding the window never
lets App Nap throttle the WebView pumps; the activity is released when the last
session ends. Hiding the window also posts a real notification through the
system notification center (macOS wording mentions the menu bar icon).

The native capture path itself has not been validated against live audio yet — only
device enumeration runs unattended, since it needs no microphone authorization.

## One-click DMG build

For a double-clickable build entry point, grant execute permission once and open
`build-dmg.command` in Finder:

```bash
chmod +x MCTier-macOS/build-dmg.command
open MCTier-macOS/build-dmg.command
```

The same entry point can be run from a terminal:

```bash
./MCTier-macOS/build-dmg.command
```

It prepares the pinned EasyTier binaries when needed, runs the full frontend and
Rust checks, builds the release DMG, verifies that the DMG is non-empty, and
prints its path. Build logs are kept under `MCTier-macOS/build-logs/` and are
ignored by Git. The one-click entry point never falls back to `--ui-only`.

The lower-level build script remains available for CI and debugging:

```bash
./MCTier-macOS/scripts/build.sh
```

The output is under `src-tauri/target/release/bundle/dmg/` for a DMG build.

To validate only the Tauri UI/app bundle without EasyTier binaries:

```bash
./MCTier-macOS/scripts/build.sh --ui-only
```

A UI-only package cannot create a virtual network, access a remote ComfyUI instance, or use MCTier port forwarding.

## Native screen capture (displays and windows)

Screen sharing, recording, and being remote-controlled use the same native
capture backend as Windows, implemented with CoreGraphics in
`src-tauri/src/modules/native_capture/macos.rs`:

- Displays come from `CGGetActiveDisplayList` (pixel dimensions via
  `CGDisplayPixelsWide/High`), windows from `CGWindowListCopyWindowInfo`
  (on-screen, layer 0, other processes, non-empty and non-transparent). Without
  Screen Recording permission macOS hides other apps' window titles, so entries
  fall back to the owning application's name.
- Every frame is a snapshot: `CGDisplayCreateImage` for displays,
  `CGWindowListCreateImage` for windows, drawn into a reused RGBA bitmap context
  scaled to the requested 720p–2160p quality. The packet contract
  (`[u32 LE width][u32 LE height][RGBA8]`) is identical to the Windows one, so
  the frontend pump, `MediaStreamTrackGenerator` bridge, and WebRTC path are
  unchanged. A unit test pins the top-down row order so the remote side never
  sees an upside-down screen.
- Screen Recording is a TCC gate: without it these APIs silently return images
  without window content, so `native_capture_sources` preflights
  (`CGPreflightScreenCaptureAccess`), asks the system to prompt once, and fails
  with a message pointing at「系统设置 › 隐私与安全性 › 屏幕录制」. A grant only
  takes effect after the app is restarted.
- Differences from Windows: cursor is not drawn into the frames, and minimizing
  a shared window ends that capture instead of freezing it. Being
  remote-controlled additionally requires an active local capture session —
  the same rule Windows enforces before injecting input.
- The legacy CoreGraphics symbols are marked obsoleted in the macOS 15 SDK but
  still resolve and work at runtime (verified present on macOS 26). If Apple
  ever removes them, the migration path is ScreenCaptureKit (macOS 13+) behind
  the same `platform` module interface.
- Live validation needs Screen Recording permission granted to the host
  terminal: `cargo test --lib native_capture -- --include-ignored`. Unpermissioned
  runs skip themselves instead of popping the system dialog.

## Troubleshooting: `cc: exit status 69` during the Rust link step

Updating macOS or Xcode revokes the previously accepted Xcode license. Until it is
accepted again, `xcrun` cannot report an SDK path and the failure surfaces far from
its cause: the final link step dies with `exit status 69` and the only clue is a
one-line license notice buried in the cargo output. The same gate also breaks
`git`, `cc`, and `lipo`, so an unrelated command may fail first.

`scripts/toolchain.sh`, sourced by `build.sh` and `build-easytier.sh`, detects this
before any compilation starts and automatically falls back to the Command Line
Tools, which are a separate install with their own license:

```text
警告: Xcode 当前不可用，已自动改用 Command Line Tools 工具链。
        SDK: /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk
        如需恢复使用 Xcode，请执行: sudo xcodebuild -license accept
```

To go back to building with Xcode itself, accept the license once (it needs your
password, so the build scripts cannot do it for you):

```bash
sudo xcodebuild -license accept
```

Both toolchains produce a valid build; the fallback exists so an Xcode update does
not block packaging.

## Troubleshooting: MCTier never opens a window

Legacy data migration runs before the WebView is created, so anything that blocks
inside it looks like the app refusing to start: clicking the icon does nothing, no
window ever appears, and `mctier.log` stays empty.

The trigger was the administrator-authorization supervisor writing its session
directory to `~/Library/Application Support/MCTier/privileged/` while application
data lives under `~/Library/Application Support/com.mctier.app/`. Shell FIFOs
cannot be opened until the other end exists, so migration walking that leftover
directory blocked forever on `open()`. Two changes close this off:

- the supervisor now creates its sessions under the application data root, so it
  no longer writes into the legacy directory at all;
- migration skips FIFO, socket, and device entries and leaves them in place
  instead of hanging on them, which also clears directories left by older builds.

## Distribution

Unsigned and unnotarized packages are for local testing. A public release needs an Apple Developer ID application certificate, hardened runtime/entitlements as required by the selected native features, and Apple notarization. macOS users may also need to grant Microphone, Screen Recording, and Accessibility permissions in System Settings.

Permission plumbing per feature: the bundled `Info.plist` (merged by tauri-build) carries `NSMicrophoneUsageDescription`, without which neither the native CoreAudio capture nor the WKWebView `getUserMedia` fallback triggers the TCC prompt and voice fails silently. Screen Recording is preflighted before capture with guidance to the settings pane. Accessibility cannot be requested programmatically — the remote-control accept dialog surfaces the localized guidance and opens the pane directly. Saved passwords live in the login keychain; re-signed development builds may re-prompt for keychain access, and a denied prompt degrades to "type the password manually" instead of breaking lobby autofill.
