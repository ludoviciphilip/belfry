# Contributing to Belfry

Thanks for considering a contribution! Belfry is a small project; keeping changes focused makes review fast.

## Build

Needs Rust (stable), Node.js and, on Linux, `webkit2gtk-4.1`.

```bash
npm ci
bash scripts/fetch-engine.sh linux     # or: windows (downloads EasyTier, checks SHA-256)
npm run build                           # builds belfryd + the bundles
```

`npm run dev` runs the app in development mode.

## Test

```bash
cargo test -p belfry-proto -p belfryd
unshare -rn bash scripts/e2e-netns.sh   # full host + guest session in network namespaces, no root
```

The same checks run in CI (`.github/workflows/build.yml`) on every push and pull request.

## Layout

| Path | What |
|---|---|
| `crates/belfry-proto` | Room code format (versioned, checksummed base32) and the API types |
| `crates/belfryd` | Background service: one EasyTier process per room, loopback JSON API with a token, Linux systemd / Windows service install |
| `src-tauri` | The app (Tauri 2) |
| `ui` | The interface: plain HTML, CSS and JavaScript, no build step |

## Pull requests

- Keep PRs small and focused; explain the "why" in the description.
- Make sure `cargo test -p belfry-proto -p belfryd` passes locally before opening the PR.
- Match the existing code style; there's no separate linter config to run.

## Questions

Open a [Discussion](https://github.com/ludoviciphilip/belfry/discussions) for questions or ideas; use [Issues](https://github.com/ludoviciphilip/belfry/issues) for concrete bugs or feature requests.
