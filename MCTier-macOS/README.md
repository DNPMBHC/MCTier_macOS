# MCTier macOS Preview

macOS adaptation contribution: [DNPMBHC/MCTier_macOS](https://github.com/DNPMBHC/MCTier_macOS).

The original MCTier author information, copyright, license boundaries, and
EasyTier third-party notices remain authoritative in the repository root.

This directory contains the macOS build and native-resource preparation scripts.

## Status

- Tauri `.app`/`.dmg` packaging is configured for macOS 11+.
- Apple Silicon (`aarch64-apple-darwin`) is the primary target; Intel uses `x86_64-apple-darwin`.
- macOS networking requires EasyTier `easytier-core` and `easytier-cli` built as matching Mach-O executables.
- The repository intentionally does not fall back to Linux ELF binaries and does not claim virtual-LAN support until those resources are verified.
- Microphone, screen recording, accessibility permissions, firewall behavior, signing, and notarization require on-device validation.

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

## Distribution

Unsigned and unnotarized packages are for local testing. A public release needs an Apple Developer ID application certificate, hardened runtime/entitlements as required by the selected native features, and Apple notarization. macOS users may also need to grant Microphone, Screen Recording, and Accessibility permissions in System Settings.
