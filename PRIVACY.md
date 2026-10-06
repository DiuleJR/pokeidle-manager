# Privacy and local data

Pokeidle Manager is designed as a local desktop app, but it communicates with remote services needed for game sessions and asset/recommendation data. This document describes behavior visible in the code; it is not a guarantee about third-party services or the operating system.

## Data stored on this device

The app uses Tauri's operating-system application-data directory. It stores a SQLite database (`pokeidle-manager.db`) containing account nick/alias and display settings, automation settings, account activity summaries (including kills, captures, XP/gold totals and error summaries), hunt state, and market rules/observations/history. It also stores cached assets.

For managed browser sessions, the app creates persistent Brave profiles in an app-data `profiles` directory, separated by account. A browser profile can contain cookies, authenticated sessions, browsing state, and other browser data. These profiles are not the same as the account records in the database. Removing an account removes manager-owned records but intentionally may keep its browser profile; deleting the database alone also does not delete those profiles.

## Network and diagnostics

The app communicates with `pokeidle.io` for game/browser/WebSocket sessions and resolves or caches game assets from remote sources. Hunt recommendations can fetch a reference script from `guiapokeidlehardtocapture.site`. These services may receive the network requests inherent in those functions and have their own privacy practices.

No app-owned analytics/telemetry pipeline was identified in this source review. Rust tracing diagnostics are emitted to stderr; the operating system or a process supervisor may capture them. Some protocol frames are sanitized before logging, but users should not share logs without reviewing them for account identifiers or sensitive data.

## Optional development dashboard

The `mobile-local-server` feature is debug-only and listens on `127.0.0.1:1421`; it serves local dashboard/API data to the development frontend. A separate optional development helper can bind Vite to a selected ZeroTier interface. That makes the development dashboard reachable by devices that can reach that interface; its Origin guard is not user authentication. These tools are not intended for production or untrusted networks.

## Deleting local data

Quit the app and its managed Brave processes before manual cleanup. To remove the manager database and cache, delete the app's Tauri application-data directory for identifier `com.pokeidle.manager` using the operating system's application-data location. This also removes other app-owned data there. To erase saved browser sessions, separately remove the corresponding `profiles` directory; doing so signs those profiles out and permanently deletes their browser state. Back up anything you need first. The app does not currently provide a verified all-data deletion flow, and uninstalling may not remove the app-data directory.

## Changes

If storage or network behavior changes, update this notice before release. This notice does not describe the independent data practices of Pokeidle, Brave, ZeroTier, or other services. The ZeroTier development helper is optional; it is not required to build or run the packaged Community edition.
