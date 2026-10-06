# Third-party notices

The dependency audit covered all 537 Cargo lockfile package records (including optional features) and all 293 npm lockfile package records (including platform-specific optional packages). Their manifests/lock metadata identify licenses; no dependency with an obvious incompatibility with the project's **GPL-3.0-only** license was identified. This is a technical inventory, not legal advice or a guarantee that every redistribution obligation has been met. Before shipping an installer, verify the notices for the exact packaged dependency set and preserve any required notices.

The project is distributed under **GPL-3.0-only**. The complete project license is in [`LICENSE`](LICENSE) and is bundled with the application as `licenses/GPL-3.0-only.txt`.

The Rust graph includes `webpki-roots` trust-root data under **CDLA-Permissive-2.0**. The applicable agreement text is included at [docs/licenses/CDLA-Permissive-2.0.txt](docs/licenses/CDLA-Permissive-2.0.txt) and configured as an app resource at `licenses/CDLA-Permissive-2.0.txt`, so a packaged copy travels with the application. Keep that resource in any distributed build embedding the `webpki-roots` data. The [official CDLA-Permissive-2.0 page](https://cdla.dev/permissive-2-0/) is also available.

The npm development dependency graph includes `caniuse-lite` browser-compatibility data identified as **CC-BY-4.0**. It is used by build tooling rather than by the application at runtime. The project source does not copy its dataset into the application; when using or redistributing that dataset, retain the package's attribution and follow the [CC-BY-4.0 terms](https://creativecommons.org/licenses/by/4.0/).

No third-party game artwork or sprites with verified redistribution permission are bundled in this repository. Runtime asset resolution and remote services remain subject to their owners' terms. See [Assets](docs/ASSETS.md).
