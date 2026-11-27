# Release Process

Regular commits pushed to `main` or feature branches do not run the desktop installer workflow. You can push development commits freely until the codebase is ready to ship.

A build runs in [build-desktop.yml](../../.github/workflows/build-desktop.yml) only when you push a version tag matching `v*` or trigger `Run workflow` manually from the GitHub Actions tab.

## Release Workflow Behavior

| Trigger | Release State | Platform Builds Run | Auto-Publish Behavior |
|---|---|---|---|
| Push to `main` or feature branch | None | None | Does not run CI installer builds |
| Manual `workflow_dispatch` | Draft Release | All 5 OS targets | Stays `Draft` after upload so you can test artifacts safely |
| Annotated tag push (`v*`) | Public Release (`latest`) | All 5 OS targets | `publish-release` marks the release public and `latest` only when all 5 build jobs succeed |

## Step-by-Step Version Release

1. Update the version string consistently in all three workspace manifests:
   - `Cargo.toml` under `[workspace.package]` (`version = "X.Y.Z"`)
   - `src-tauri/tauri.conf.json` (`"version": "X.Y.Z"`)
   - `package.json` (`"version": "X.Y.Z"`)
2. Move finalized items in [CHANGELOG.md](../../CHANGELOG.md) from `[Unreleased]` into a dated `[X.Y.Z] - YYYY-MM-DD` section.
3. Run the local pre-release verification suite:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test -p kivi-core -p kivi-checksum -p kivi-formats -p kivi-flashing -p kivi-protocol -p kivi-j2534-bridge -p kivituner
   npx tsc --noEmit
   npm test
   npm run build
   ```
4. Commit the version bump and changelog update, then push to `main`:
   ```bash
   git add Cargo.toml src-tauri/tauri.conf.json package.json CHANGELOG.md
   git commit -m "chore(release): prepare vX.Y.Z"
   git push origin main
   ```
5. Create and push an annotated version tag:
   ```bash
   git tag -a vX.Y.Z -m "kiviTuner vX.Y.Z"
   git push origin vX.Y.Z
   ```

## Pipeline Stages on Tag Push

1. `prepare-release` creates or reuses a single Draft Release matching the pushed tag and passes its `release_id` downstream so parallel build runners never race.
2. Five `build-tauri` runners compile and test in parallel, producing Windows x64 (`.exe`, `.msi` with bundled `kivi-j2534-bridge-x86.exe`), Windows x86 (`.exe`, `.msi`), Linux x86_64 (`.deb`, `.rpm`, `.AppImage`), macOS Apple Silicon (`.dmg`, `.app.tar.gz`), and macOS Intel (`.dmg`, `.app.tar.gz`).
3. `publish-release` transitions the release from `Draft` to public and sets it as the repository `latest` release once all five platform builds pass.

If any platform fails before bundling, `publish-release` stays skipped and the partial release remains a `Draft` until fixed and re-tagged.
