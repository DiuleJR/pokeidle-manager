# Pokeidle Manager

Pokeidle Manager is an unofficial desktop companion for organizing multiple Pokeidle accounts and viewing account, hunt, inventory, automation, and market information. It is not affiliated with, endorsed by, or operated by the Pokeidle game or its rights holders.

This repository is the canonical community edition source. It is not affiliated with or endorsed by the game. Official downloads and support channels have not been established; builds made from this source are community builds, not official game releases.

## Build locally

The desktop application uses Tauri 2, Rust, Node.js, and npm. On Windows, install the prerequisites documented by the official [Tauri v2 prerequisites guide](https://v2.tauri.app/start/prerequisites/), then run:

```powershell
npm ci
npm run check
npm run build
npm run tauri:build:community
```

The Community edition is built without a commercial licensing service. The optional mobile dashboard server is development-only; do not enable it in a packaged build.

## Data and network

The app stores its SQLite database, cached assets, and persistent per-account Brave browser profiles in the operating system's Tauri application-data directory. Browser profiles may contain login cookies and other session data. Removing an account from the app does not necessarily remove its browser profile. See [Privacy](PRIVACY.md) for details and deletion cautions.

The app connects to Pokeidle's site/services for game data and sessions, and may fetch assets at runtime. Hunt recommendations use a community reference script fetched from `guiapokeidlehardtocapture.site`; the repository references the source but does not redistribute that script. Game sprites and unapproved third-party artwork are not bundled. See [Assets](docs/ASSETS.md).

## Contributing and security

Please read [Contributing](CONTRIBUTING.md), the [Code of Conduct](CODE_OF_CONDUCT.md), [Security](SECURITY.md), and [Trademarks](TRADEMARKS.md). Official contact and support channels have not yet been established; do not infer or rely on unofficial channels.

## License

Project code is offered under **GPL-3.0-only**; see [LICENSE](LICENSE). Third-party components and data may have separate terms. The current dependency-license review is not complete; see [Third-party notices](THIRD_PARTY_NOTICES.md). No legal conclusion is implied.
