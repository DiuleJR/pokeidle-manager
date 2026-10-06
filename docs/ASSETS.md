# Asset inventory and provenance

This inventory distinguishes project-owned artwork from material supplied or fetched by other projects and services. No item intended for inclusion in this repository has `UNKNOWN` provenance. Runtime references are not a license to redistribute the referenced content.

| Classification | Asset / source | Use | License / distribution status |
| --- | --- | --- | --- |
| `PROJECT_ORIGINAL` · `PROJECT_SVG` | `src-tauri/icon-source.svg` | Source artwork for the application icon | Created for this project; distributed under GPL-3.0-only with project code. |
| `PROJECT_ORIGINAL` · `PROJECT_SVG` · `OPENAI_GENERATED` | 44 SVGs in `src/assets/ui-icons/`, used through `src/components/UiIcon.tsx` | Desktop/mobile interface: navigation, account and Pokémon details, hunt stats, currency, automation, market, and window controls | Original project interface artwork generated for this project with AI assistance; no third-party icon pack, font, conversion service, or remote SVG is used. Distributed under GPL-3.0-only with project code. |
| `PROJECT_ORIGINAL` · `PROJECT_SVG` | Generated platform icon files under `src-tauri/icons/` | Tauri application and installer icons | Build derivatives of the project icon source above; do not edit these independently of the SVG source. |
| `PROJECT_ORIGINAL` | Interface source and visual treatments under `src/` | Application UI | Project source distributed under GPL-3.0-only. |
| `GAME_RUNTIME_ASSET` | Game catalog and images fetched from `pokeidle.io` endpoints; image bytes are cached under the Tauri AppData `assets/files` directory and catalog metadata in `assets/catalog-v1.json` | Pokémon, item, and game visuals in account, inventory, and hunt screens | Third-party game content; fetched and cached at runtime, not redistributed as repository files. Runtime use remains subject to the source owners' terms. |
| `EXTERNAL_RUNTIME_REFERENCE` | `https://guiapokeidlehardtocapture.site/data.js?v=32` | Hunt recommendation data | Fetched at runtime; response is parsed as `window.PI_DATA = JSON` (not evaluated as code), retained in a process-local `OnceCell`, and not persisted. The repository does not redistribute the response. Source-site terms and redistribution rights remain separate from this project. |
| `OPEN_SOURCE_DEPENDENCY` | Packages listed in `package-lock.json` and `src-tauri/Cargo.lock` | Application and build dependencies | Third-party licenses and relevant notices are summarized in [Third-party notices](../THIRD_PARTY_NOTICES.md). |

## Excluded material

Design-reference folders and unverified converted images from earlier private development are not part of this repository. Game artwork, Pokémon sprites, item sprites, and backgrounds with no verified redistribution permission are also excluded. Do not re-add these files without recording their source, author, license or written permission, attribution requirements, and any modifications.

## Runtime boundaries

Game catalog metadata and sprite/image bytes are fetched from game services and cached under the operating-system application-data directory; this does not make those assets repository content or grant redistribution rights. Hunt data comes from the external JSON-like reference endpoint, is parsed rather than executed, held only in process memory, and not cached to disk. Neither behavior endorses the external sources or changes their terms.

The generated application icon has no game artwork or third-party marks. Its source is `src-tauri/icon-source.svg`; platform-specific files in `src-tauri/icons/` are generated derivatives.
