# Releasing Icon Crafter

Icon Crafter ships through GitHub Actions. Three workflows cooperate; this page
is the operator reference (the YAML files carry the same notes inline).

| File | Purpose |
|---|---|
| `.github/workflows/ci.yml` | Every PR and push to `main`: Rust fmt/clippy/tests, backend (incl. `s3`/`otel` feature checks), UI typecheck/tests/build, production packaging smoke test. |
| `.github/workflows/version.yml` | **Version controller.** Raises the version, updates the changelog and tags `vX.Y.Z`. |
| `.github/workflows/ship.yml` | Builds the Windows installers for a tag and publishes the GitHub Release. |
| `.github/workflows/auto-ship.yml` | Patch-ships direct pushes to `main` (skips docs/CI-only changes and release commits). |
| `.github/workflows/security.yml` | Dependabot-adjacent checks: gitleaks, `cargo audit`, `cargo deny`, repo hygiene. |
| `.github/dependabot.yml` | Weekly Rust/Node/Actions updates and monthly Docker updates. |

## Normal release (reviewed changes)

1. Merge the PR(s) you want to ship into `main`.
2. Go to **Actions → Release version → Run workflow** and enter the new version
   (`1.4.0` or `1.4.0-beta.1`), choose *prerelease* if needed.
3. The workflow:
   - validates the version and refuses to go backwards or equal,
   - runs `cargo set-version --workspace`,
   - syncs `apps/ui/package.json` and `apps/desktop/tauri.conf.json`,
   - closes `CHANGELOG.md`'s `## [Unreleased]` section under
     `## [X.Y.Z] - <date>` and adds the compare links,
   - commits as `github-actions[bot]` with `chore(release): X.Y.Z`,
   - creates and pushes the annotated tag `vX.Y.Z`,
   - dispatches *Ship* for that tag.
4. *Ship* builds the production UI bundle, compiles the desktop app with
   `--features custom-protocol` (embedded UI; **required** for a working
   standalone app) and `tauri build --bundles nsis,msi`, then publishes the
   release with:
   - `Icon Crafter_X.Y.Z_x64-setup.exe` (NSIS, per-user friendly),
   - `Icon Crafter_X.Y.Z_x64_en-US.msi` (per-machine MSI),
   - `iconcrafter-X.Y.Z-windows-x64-portable.exe`,
   - `SHA256SUMS.txt`, and release notes generated from the changelog.

## Auto ship (direct pushes to `main`)

Pushing directly to `main` starts `.github/workflows/auto-ship.yml`:

- commits starting with `chore(release):`, `docs:`, `ci:` or `chore:` are
  ignored (no release loop),
- pushes that only touch Markdown/`docs/`/`.github/`/licence files are
  ignored,
- otherwise the next patch version is bumped, tagged and shipped.

If the bot cannot push (e.g. the repository is read-only for the token), the
workflow fails loudly instead of silently skipping the release.

## Manual ship (re-tagging an existing commit)

**Actions → Ship → Run workflow** with the `tag` input (e.g. `v0.1.1`) and
`prerelease` if desired. Handy after fixing infrastructure without changing
code.

## Required secrets and permissions

Set these in **Settings → Secrets and variables → Actions** (all optional, the
workflows fall back to `GITHUB_TOKEN` where possible):

| Secret | Used for |
|---|---|
| `RELEASE_TOKEN` | Pushing the version commit/tag past branch protection in *Version*. |
| `GH_RELEASE_TOKEN` | Publishing releases in *Ship* (needs `contents: write`). |
| `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Signing the updater artifacts (optional until the updater lands). |

For organisation repositories, enable **Settings → Actions → General →
Allow GitHub Actions to create and approve pull requests** if you want
Dependabot auto-merge later.

## Environment requirements

- Windows is the only packaging target today (NSIS + MSI, WebView2 runtime is
  preinstalled on Windows 10/11).
- The release build needs the UI built first; *Ship* does this automatically.
- Installers are **unsigned**: Windows SmartScreen warns on first run until a
  code-signing certificate is configured (`TAURI_SIGNING_*`/`bundle.windows.certificateThumbprint`).

## Rolling back

- Revert the offending commits and ship a new patch via *Auto ship*.
- To pull a broken release, edit it on the Releases page (mark as pre-release)
  rather than deleting the tag; the tag stays reproducible.

## Local equivalents

```powershell
pnpm --dir apps/ui build
cargo build -p iconcrafter-desktop --release --features custom-protocol
.\tools\package-windows.ps1 -Bundle nsis      # or msi / both
```
