# Contributing

Thanks for helping improve Pokeidle Manager. The source is maintained as the canonical community edition. A public issue tracker, pull-request host, and support channel will be linked here once their addresses are established.

## Before proposing a change

- Keep changes focused and explain user impact and any compatibility or data-migration effects.
- Do not commit account databases, browser profiles, logs, cookies, tokens, screenshots with personal data, local environment files, or private recovery material.
- Do not add game artwork, sprites, logos, or other third-party assets without documented redistribution permission. See [Assets](docs/ASSETS.md).
- Preserve local-first behavior and clearly disclose any new external network request or collection of data.
- Never expose the development mobile server or Vite helper to an untrusted network.

## Local checks

Use a supported Node.js/npm and Rust toolchain, then run:

```powershell
npm ci
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

For a packaged Windows Community build, use the repository's explicit community-build command and verify the resulting artifact locally.

## Submitting

Once a public hosting location is configured, use its documented contribution workflow. Include tests or manual verification, note limitations, and keep commits free of generated build output and personal data. By submitting a contribution, you agree it is offered under the project license unless the maintainers document another arrangement.
