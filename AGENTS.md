# Vessel development

- Follow the owner's workflow: after a requested change is complete and verified, make a focused commit and push the current branch to `origin`, unless the owner asks otherwise. Keep the repository private. Never force-push or amend existing commits without an explicit request.
- Review status/diff before committing. Keep credentials, downloaded runtimes, build outputs, caches, and test artifacts out of commits.
- Frontend checks: `npm run check`, `npm run test:ui`, and a production build. For native changes also run Rust formatting, Clippy with warnings denied, and appropriate Rust tests. Package macOS changes with `npm run tauri -- build --bundles app`.
- Playback and provider state belong in Rust. Appearance preferences belong in the frontend. Duskfox is the default palette; planned additional themes should reuse semantic tokens under `src/lib/themes/`.
- Preserve existing collection data and user preferences. Record actual measurements and distinguish mocked interface checks from live provider/native verification.
