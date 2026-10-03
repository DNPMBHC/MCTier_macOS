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
- The macOS main window uses native decorations and the standard left-side red/yellow/green traffic lights. The yellow button minimizes to the Dock and the green button zooms or enters native full screen; the main page uses a desktop-sized responsive layout instead of the compact 320px form.
- Secondary overlays (screen viewer, danmaku, and game HUD) retain their dedicated transparent/overlay behavior.
- The traffic-light, Cmd+W/Cmd+Q, multi-monitor restore, Retina sizing, and full-screen behavior require on-device validation.

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

## Distribution

Unsigned and unnotarized packages are for local testing. A public release needs an Apple Developer ID application certificate, hardened runtime/entitlements as required by the selected native features, and Apple notarization. macOS users may also need to grant Microphone, Screen Recording, and Accessibility permissions in System Settings.
