# Security policy

No dedicated security contact or supported release channel has been established yet. Do not send vulnerability details to an address found outside this repository, and do not include secrets or personal account data in public issues or logs. Maintainers should add a verified private reporting channel before publishing supported binaries.

For a report, include the affected version/commit, operating system, reproduction steps, and impact. Redact account names, browser cookies, tokens, database contents, and identifying network details. Please allow maintainers time to investigate and prepare a fix before public disclosure.

## Safety notes

- Browser profiles persist under the app-data directory and can contain authenticated session cookies. Treat that directory as credential-bearing data.
- The optional mobile dashboard server is development-only and binds to loopback. Do not expose it to a LAN or the internet.
- The optional ZeroTier development helper deliberately binds the Vite dev server to a selected ZeroTier interface. Anyone with network reachability to that interface may be able to reach the development UI; the Origin check is not authentication. Use only on a trusted network and stop it when finished.
- Rust diagnostics are written to stderr through the tracing subscriber. The operating system or a launcher may capture them. Sanitization exists for some inspected protocol frames, but do not assume logs never contain sensitive information.
