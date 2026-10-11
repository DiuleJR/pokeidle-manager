# Linux Port Audit

## Baseline

- Release candidate: `v0.1.0-rc.3`
- Expected commit: `3b30de0d7a4edd7e2c1d13cb592c9b5a4ae4ebad`
- Verified: yes; the public tag resolves to that commit, and `origin/main` pointed to it when this branch was created.
- Research branch: `research/linux-port`
- Scope: initial portability audit and experimental Linux build only. Linux is not an officially supported platform in this phase.

## Target

Linux x86_64, using Ubuntu 22.04 as the CI baseline for Linux Mint / Debian / Ubuntu investigation. The build has not been smoke-tested on a physical Linux Mint desktop.

## Already portable

- Rust process APIs and filesystem storage use `std::path::PathBuf` and Tauri's `app_data_dir()`; the database and asset directory are constructed beneath that OS-specific app-data location.
- SQLite uses `rusqlite` with `bundled`; no system SQLite dependency or schema change was needed.
- HTTP and WebSocket clients enable rustls; CDP remains bound to `127.0.0.1`.
- The Windows-only process creation flags and `taskkill.exe` code are cfg-gated. The frontend checks and production build pass on a case-sensitive Ubuntu filesystem.
- Linux CI installs Tauri 2's documented GTK/WebKitGTK dependencies plus AppIndicator, xdotool, OpenSSL headers, librsvg and `patchelf`.

## Compile blockers

None found in the RC3 baseline. No source correction was needed to compile on Linux.

The first Ubuntu 22.04 run passed `npm run check`, `npm run build`, `cargo fmt`, `cargo check`, `cargo clippy` and `cargo test` with the community-build feature set. The `cargo check` step was deliberately run before any Rust changes; it passed.

## Runtime blockers

- **Brave discovery and executable validation are Windows-only.** `src-tauri/src/browser.rs` searches Windows environment variables and `brave.exe` locations, then validates the `MZ` PE signature. Consequently, account login, opening and reconnecting through Browser are unavailable on Linux even though the app compiles. `src-tauri/src/lib.rs` calls this manager from the account commands. No Linux Brave implementation was added.
- **Process-tree fallback differs.** On Windows the recovery fallback uses `taskkill /T`; on non-Windows it calls `Child::kill()` on the owned root process only. If normal CDP `Browser.close` fails, Chromium descendants may outlive that root. Do not kill by process name; any future process-group solution must preserve ownership boundaries.
- **Tray and custom window behavior need desktop testing.** `tray-icon` is enabled and the app creates a tray icon/menu during startup. AppIndicator headers are installed for CI, but minimize/restore/close and tray visibility vary by Linux desktop shell. The undecorated, fixed-size window may also behave differently across window managers.

## Packaging blockers

None found for the initial Ubuntu 22.04 build. Tauri successfully generated both experimental bundle formats:

- Runner path: `src-tauri/target/release/bundle/deb/Pokeidle Manager_0.1.0_amd64.deb`
- Runner path: `src-tauri/target/release/bundle/appimage/Pokeidle Manager_0.1.0_amd64.AppImage`
- `.deb` size: `18,825,794` bytes (about 18.0 MiB).
- AppImage size: `96,831,992` bytes (about 92.4 MiB).
- The CI uploads them as `pokeidle-manager-linux-experimental`, retained for 7 days. They are not attached to a GitHub Release.

## Windows-specific files and impact

| File                                        | Windows dependency / assumption                                                 | Linux impact                                                                                       | Compile?                                                   | Runtime / packaging impact                                                                    | Action in this phase                                                           |
| ------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `src-tauri/src/browser.rs`                  | Windows install paths, `.exe`, `MZ` validation, `SystemRoot` and `taskkill.exe` | Detector cannot find or accept Linux Brave; non-Windows emergency kill only targets the child root | No; Windows-only flags and `taskkill` branch are cfg-gated | Blocks account Browser/login; possible orphaned Chromium descendants during emergency cleanup | Document only; Brave/CDP port deferred                                         |
| `src-tauri/src/lib.rs`                      | Account commands instantiate `WindowsBraveManager`                              | All browser account operations depend on the Windows-only detector                                 | No                                                         | Same Brave runtime blocker                                                                    | No behavior change                                                             |
| `src-tauri/Cargo.toml`                      | Direct `windows-sys` dependency                                                 | No Linux compile failure observed; no direct source usage found                                    | No                                                         | Unneeded direct dependency, not a package blocker                                             | Not moved; consider removing in a separate cleanup after confirming Windows CI |
| `src-tauri/tauri.conf.json`                 | NSIS/WiX settings and `.ico` alongside PNG                                      | Windows bundler settings are target-specific; PNG icon is available to Linux                       | No                                                         | Linux window chrome/tray must be checked on real desktops                                     | No visual or bundle config changes                                             |
| `.github/workflows/release.yml`             | Windows-only NSIS/MSI release process                                           | No Linux release workflow                                                                          | No                                                         | Intentionally does not publish Linux artifacts                                                | Leave Windows release pipeline unchanged                                       |
| `.github/workflows/ci.yml`                  | Existing Windows jobs                                                           | No Linux test coverage before this change                                                          | No                                                         | Now includes a parallel Ubuntu 22.04 experimental job and temporary package artifacts         | Add Linux CI without replacing Windows jobs                                    |
| `src-tauri/src/assets.rs`, `persistence.rs` | No Windows path API found                                                       | Uses app-data path and `PathBuf::join`                                                             | No                                                         | Assets and SQLite compile with bundled SQLite                                                 | No changes                                                                     |
| `src-tauri/Cargo.toml` network dependencies | No Windows-only TLS assumptions                                                 | HTTP/WebSocket use rustls                                                                          | No                                                         | Linux compilation passed                                                                      | No changes                                                                     |

## CI status

The initial baseline run (`Verificações automatizadas`, run `38125952981`) and final run (`38126909855`) completed successfully on both Windows and Ubuntu 22.04. Linux frontend, formatting, check, clippy, tests, `.deb`/AppImage bundling and artifact upload all passed. The final run prints the exact package paths and byte sizes above.

The Linux job uses no game credentials, cookies or real browser profiles and does not start a game login. It does not open network debugging beyond `127.0.0.1`.

## First build

- Baseline Rust check: passed on Ubuntu 22.04, with no compile blocker and no Rust edits.
- `.deb`: generated, experimental; filename and runner path are listed above.
- AppImage: generated, experimental; filename and runner path are listed above.
- GitHub Release/tag: not created or changed.

## Brave / Browser

Brave for Linux has not been implemented. A later platform abstraction should isolate only executable discovery and validation (Windows paths/PE versus Linux packages/ELF), while preserving the shared profile, CDP target ownership, WebSocket observer, handoff, Background behavior and loopback-only security model. Validate process-group cleanup separately before changing lifecycle behavior.

## Tray / UI differences

Tray visibility and minimize/restore/close behavior still require manual checks on Linux Mint with its actual desktop environment. Also verify the undecorated fixed-size window and asset-protocol scope in a packaged install. This CI build proves compilation and bundling, not GUI behavior.

## README

The RC3 baseline README still recommends RC2 and links its downloads. This is a separate documentation inconsistency; it was not changed as part of the Linux technical PR. The README does not claim Linux support.

## Next steps

1. Review this experimental PR and the CI results; do not publish the bundles as a release.
2. In a separate phase, implement and test Linux Brave detection/validation with Linux-specific tests while keeping the existing Windows behavior intact.
3. Test account lifecycle, tray, window behavior and installed `.deb`/AppImage on Linux Mint before making any support claim.

## CI / PR references

- Experimental draft PR: https://github.com/DiuleJR/pokeidle-manager/pull/13
- Final experimental workflow run: https://github.com/DiuleJR/pokeidle-manager/actions/runs/38126909855
