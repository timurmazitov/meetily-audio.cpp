# Research: Tauri v2 Auto-Updater Setup for Samobranka

Date: 2026-11-17
Scope: how to correctly implement Tauri auto-updates for this repo, researched against primary sources only (v2.tauri.app, github.com/tauri-apps/*, docs.rs).
Target repo: `https://github.com/timurmazitov/samobranka`

---

## 1. Version inventory (exact versions in this repo)

| Component | Version | Source |
|---|---|---|
| `tauri` crate | `2.6.2` | `frontend/src-tauri/Cargo.toml:154,168` |
| `tauri-build` | `2.3.0` | `frontend/src-tauri/Cargo.toml:56` |
| `tauri-plugin-updater` (Rust) | `2.3.0` (`"2.3.0"` → semver `^2.3.0`) | `frontend/src-tauri/Cargo.toml:159` |
| `@tauri-apps/api` | `^2.6.0` | `frontend/package.json:59` |
| `@tauri-apps/plugin-updater` (JS) | `^2.3.0` → **resolved 2.10.1** in `frontend/pnpm-lock.yaml` (line 1692) | `frontend/package.json:65` |
| `@tauri-apps/cli` | `^2.1.0` | `frontend/package.json:101` |
| App version | `0.5.0` | `frontend/src-tauri/tauri.conf.json:5` |

This is **Tauri v2** (not v1). All config field names below are the v2 names. The crate `tauri-plugin-updater 2.3.0` was published 2024-12-10 (https://docs.rs/crate/tauri-plugin-updater/2.3.0).

**Lockfile pitfall:** `frontend/src-tauri/Cargo.lock` is **gitignored** in this repo (`git check-ignore` confirms). `tauri-plugin-updater = "2.3.0"` in Cargo.toml therefore floats to the newest 2.x on every fresh `cargo build` (CI included). The plugins-workspace `v2` branch README states: *"This plugin requires a Rust version of at least **1.90**"* (https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/updater), while this repo declares `rust-version = "1.77"` (`Cargo.toml:9`). A fresh resolve can pull a plugin version whose MSRV exceeds the repo's declared one. Consider committing `Cargo.lock` or pinning the plugin.

## 2. Current state of the repo (why updates don't work)

All of the following were verified by reading the files:

1. **Updater config removed.** `frontend/src-tauri/tauri.conf.json` has **no `plugins.updater` section at all** (no `endpoints`, no `pubkey`), and `"createUpdaterArtifacts": false` (line 97). ADR 0002 (`docs/adr/0002-rebrand-to-samobranka.md:14`, in Russian) records why: the old endpoint pointed at a third-party repo and could serve incompatible updates; re-enabling under our own GitHub infra is an explicit follow-up task ("новый endpoint + новая minisign-пара, permission `updater:default` вернуть в capabilities").
2. **Plugin not registered in Rust.** `frontend/src-tauri/src/lib.rs:576-578` has the registration commented out:
   ```rust
   // Updater disabled per ADR 0002 (config section removed; re-enable with
   // a new endpoint + minisign key pair and the updater capability).
   // .plugin(tauri_plugin_updater::Builder::new().build())
   ```
3. **No updater permission.** Capabilities are declared inline in `tauri.conf.json` (`app.security.capabilities`, lines 40-79). There is no `updater:default` (or any `updater:*`) permission, and there is no `frontend/src-tauri/capabilities/` directory. Without the permission, the frontend `check()` command is denied by the Tauri capability system (https://v2.tauri.app/plugin/updater/#permissions).
4. **Frontend code is already written** and uses the correct v2 API: `frontend/src/services/updateService.ts` (`check()` from `@tauri-apps/plugin-updater`), `frontend/src/components/UpdateDialog.tsx` (`downloadAndInstall` with progress events), `frontend/src/components/About.tsx:23-39` ("Check for Updates" button → `updateService.checkForUpdates(true)`). This code currently fails at runtime because of items 1-3 — the "Check for Updates" button throws and shows the toast at `About.tsx:35`.
5. **CI is pre-wired for re-enablement.** The workflows already reference `secrets.TAURI_SIGNING_PRIVATE_KEY` / `secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (`.github/workflows/build.yml:681-682,762-763`, and the platform workflows). `.github/workflows/release.yml:119-172` contains a `validate-release` job that **automatically switches into updater mode when `plugins.updater.pubkey` appears in `tauri.conf.json`**: it decodes the pubkey, installs `minisign`, verifies signatures via `scripts/check-release-assets.py`, and uploads a generated `latest.json` to the release. `scripts/generate-update-manifest-github.js` and `scripts/test-update-locally.js` also exist as local tooling.
6. **The Windows conf** `frontend/src-tauri/tauri.windows.conf.json` only adds bundle resources; it contains no updater config.

## 3. Correct setup for this exact version (Tauri v2, updater plugin 2.x)

### 3.1 `tauri.conf.json` changes (v2 field names)

Source: https://v2.tauri.app/plugin/updater/ (Tauri Configuration section) and the `Config` struct of `tauri-plugin-updater 2.3.0` at https://docs.rs/tauri-plugin-updater/2.3.0/tauri_plugin_updater/struct.Config.html:

```rust
pub struct Config {
    pub dangerous_insecure_transport_protocol: bool,
    pub endpoints: Vec<Url>,
    pub pubkey: String,
    pub windows: Option<WindowsConfig>,
}
```

Note: **there is no `active` field in v2** (that was v1). In v2 the updater is "active" when the plugin is registered, the capability is granted, and `endpoints` + `pubkey` are set.

```jsonc
{
  "bundle": {
    "createUpdaterArtifacts": true        // was: false  (https://v2.tauri.app/plugin/updater/#building)
  },
  "plugins": {
    "updater": {
      "pubkey": "CONTENT OF public key from `tauri signer generate` — NOT a file path",
      "endpoints": [
        "https://github.com/timurmazitov/samobranka/releases/latest/download/latest.json"
      ],
      "windows": {
        "installMode": "passive"          // optional; "passive" is the default (https://v2.tauri.app/plugin/updater/#installmode-on-windows)
      }
    }
  }
}
```

Config-field facts (https://v2.tauri.app/plugin/updater/#tauri-configuration):

- `pubkey` "has to be the public key generated from the Tauri CLI … It **cannot** be a file path!"
- `endpoints` "must be an array of endpoint URLs as strings. TLS is enforced in production mode. Tauri will only continue to the next url if a non-2XX status code is returned!"
- `dangerousInsecureTransportProtocol` allows non-HTTPS endpoints; don't use for GitHub.
- Dynamic URL variables: `{{current_version}}`, `{{target}}` (`linux`/`windows`/`darwin`), `{{arch}}` (`x86_64`/`i686`/`aarch64`/`armv7`). Not needed for the static GitHub `latest.json` URL.

### 3.2 Rust registration (`frontend/src-tauri/src/lib.rs`)

Official setup (https://v2.tauri.app/plugin/updater/#setup and plugin README https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/updater):

```rust
tauri::Builder::default()
    .setup(|app| {
        #[cfg(desktop)]
        app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
        Ok(())
    })
```

For this codebase, that means uncommenting/restoring `.plugin(tauri_plugin_updater::Builder::new().build())` in the builder chain at `lib.rs:571-579`. The dependency already exists (`tauri-plugin-updater = "2.3.0"`, `Cargo.toml:159`).

Optional runtime configuration (Rust-only): `app.updater_builder().endpoints(...)`, `.pubkey(...)`, `.version_comparator(...)`, `.on_before_exit(...)` — https://v2.tauri.app/plugin/updater/#runtime-configuration.

### 3.3 Capability / permission

Source: https://v2.tauri.app/plugin/updater/#permissions. Add `"updater:default"` to the permission array of the inline capability in `tauri.conf.json` (or a `capabilities/*.json` file). `updater:default` grants `allow-check`, `allow-download`, `allow-install`, `allow-download-and-install` — i.e. the full workflow from checking to installing.

### 3.4 JavaScript API (verbatim signatures for 2.x)

Source: https://v2.tauri.app/reference/javascript/updater/ (generated from guest-js; installed version 2.10.1 is API-compatible with these signatures, all `Since 2.0.0`).

```ts
function check(options?: CheckOptions): Promise<Update | null>;
```
"Checks the configured endpoints for an available update… A promise resolving to the available `Update`, or `null` when no update is available." `Update.available` is **deprecated** ("always true, check if the return value is `null` instead") — this repo's `updateService.ts:66` uses `update?.available`, which still works but should migrate to a null check.

```ts
interface CheckOptions {
  headers?: HeadersInit;
  proxy?: string;
  target?: string;
  timeout?: number; // ms
}

class Update {
  version: string;
  currentVersion: string;
  date?: string;      // RFC 3339
  body?: string;      // release notes
  rawJson: Record<string, unknown>;
  close(): Promise<void>;                                    // release the Rust-side resource if not installing
  download(onEvent?, options?: DownloadOptions): Promise<void>;
  install(options?: InstallOptions): Promise<void>;           // must follow download()
  downloadAndInstall(onEvent?, options?: DownloadOptions & InstallOptions): Promise<void>;
}

type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };
```

Platform notes from the API docs: on **Windows**, `install()`/`downloadAndInstall()` **exit the app** after launching the updater installer; on **macOS/Linux** you must relaunch the app yourself (`relaunch()` from `@tauri-apps/plugin-process`) to run the new version. Restarting immediately is not required — prompting the user is fine (https://v2.tauri.app/plugin/updater/#checking-for-updates).

Manual "Check for Updates" flow (matches what the repo's `UpdateDialog.tsx` already does, and is the officially documented pattern):

```ts
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

const update = await check();               // null = up to date
if (update) {
  await update.downloadAndInstall((event) => { /* Started/Progress/Finished */ });
  await relaunch();                          // macOS/Linux; Windows already exited
}
```

Error-handling notes for the button: `check()` rejects with the raw plugin error (e.g. endpoint fetch failure, invalid manifest). There is no built-in "user rejected update" concept — `close()` the `Update` if the user declines. The 24h skip cache in `updateService.ts` is app-side only; `check()` always hits the endpoint.

### 3.5 Static JSON update manifest (`latest.json`)

Source: https://v2.tauri.app/plugin/updater/#static-json-file. Served from the GitHub release as asset `latest.json`; the app fetches it via `…/releases/latest/download/latest.json`.

```json
{
  "version": "0.6.0",              // SemVer, optional leading "v"
  "notes": "release notes",
  "pub_date": "2026-11-17T00:00:00Z",  // RFC 3339, optional
  "platforms": {
    "linux-x86_64":  { "signature": "<content of .AppImage.sig>",       "url": "<asset url>" },
    "windows-x86_64":{ "signature": "<content of .msi.sig or setup.exe.sig>", "url": "<asset url>" },
    "darwin-aarch64":{ "signature": "<content of .app.tar.gz.sig>",     "url": "<asset url>" },
    "darwin-x86_64": { "signature": "...", "url": "..." }
  }
}
```

Hard requirements and gotchas, verbatim from the docs:

- Platform keys are `OS-ARCH` with `OS ∈ {linux, darwin, windows}`, `ARCH ∈ {x86_64, aarch64, i686, armv7}`.
- `signature` is **"The content of the generated `.sig` file… A path or URL does not work!"**
- Required keys: `"version"`, `"platforms.[target].url"`, `"platforms.[target].signature"`.
- **"Tauri will validate the whole file before checking the version field, so make sure all existing platform configurations are valid and complete."** — e.g. a `darwin-x86_64` entry pointing at an asset that doesn't exist breaks the check even for Linux users.

### 3.6 Signing keys

Source: https://v2.tauri.app/plugin/updater/#signing-updates and CLI reference https://v2.tauri.app/reference/cli/#signer.

- "Tauri's updater needs a signature to verify that the update is from a trusted source. **This cannot be disabled.**"
- Generate the minisign key pair with the project's own CLI (`@tauri-apps/cli ^2.1.0` is fine; signing format is stable across 2.x):
  ```
  pnpm tauri signer generate -w ~/.tauri/samobranka.key
  ```
  (`-w/--write-keys` writes the private key; a password can be set interactively or via `-p`.)
- Outputs: a **private key** (keep secret; losing it means no more updates for installed users) and a **public key** (paste its content into `plugins.updater.pubkey`).
- At build time the private key must be in env vars — "`.env` files do *not* work!":
  ```
  export TAURI_SIGNING_PRIVATE_KEY="path-or-content"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""   # if the key has a password
  ```
  (`signer sign` also supports `TAURI_SIGNING_PRIVATE_KEY_PATH`; see https://v2.tauri.app/reference/cli/#signer-sign.)
- With `bundle.createUpdaterArtifacts: true` and those env vars set, `tauri build` emits the signed updater artifacts (https://v2.tauri.app/plugin/updater/#building):
  - Linux: `myapp.AppImage` + `myapp.AppImage.sig`
  - macOS: `myapp.app.tar.gz` + `myapp.app.tar.gz.sig`
  - Windows: `myapp-setup.exe` + `.sig`, and `myapp.msi` + `.sig`

### 3.7 `tauri-apps/action` (GitHub Action) — version-specific input names

The repo pins **`tauri-apps/tauri-action@v0`** (`.github/workflows/build.yml:670`, plus the platform workflows). Input names differ between v0 and v1:

- **v0** (used here) — from `action.yml` on the v0 branch (https://github.com/tauri-apps/tauri-action/blob/v0/action.yml):
  - `includeUpdaterJson` — "Whether to upload a static JSON file for the updater using GitHub Releases as the CDN", **default `true`**.
  - `updaterJsonPreferNsis` — use NSIS `setup.exe` instead of WiX `.msi` for the updater JSON when both exist; v0 default `false` (MSI preferred).
  - `tauriScript`, `tagName`, `releaseName`, `releaseDraft`, `releaseId`, `args`, `projectPath` as used in this repo.
- **v1** (current `dev`) renamed the option to `uploadUpdaterJson` (https://github.com/tauri-apps/tauri-action#usage). If the repo ever upgrades the action pin, `includeUpdaterJson` in `build.yml:691` must be renamed.

How `latest.json` gets published (per action docs, https://github.com/tauri-apps/tauri-action#readme and https://v2.tauri.app/distribute/pipelines/github/): each matrix job uploads its bundles and `.sig` files to the release; the action then generates and uploads `latest.json` pointing at those assets (assuming the updater is configured and signing env vars are present). Required env: `GITHUB_TOKEN` with `contents: write` (https://v2.tauri.app/distribute/pipelines/github/#troubleshooting — otherwise "Resource not accessible by integration").

Documented pitfalls relevant to this repo's `build.yml`:

- "`releaseId` set but `tagName` not set → `latest.json` will point to `releases/latest/download/<bundle>` instead of the tag" (action README). This repo sets `tagName` for macOS/Windows but intentionally not for the Linux job (build.yml:692-694); combined with the release.yml flow (draft release, then validate, then publish) that is workable, but be aware `releases/latest/download` resolves to the newest **published** release — a still-draft or prerelease release is not "latest" on GitHub. Keep prereleases in mind: GitHub's `…/releases/latest` excludes drafts and prereleases.
- The current Linux job *manually* signs and uploads the `.deb` + `.AppImage` (build.yml:757-779) and `release.yml` generates `latest.json` itself via `scripts/check-release-assets.py`. That means two writers of updater metadata must not fight: either let tauri-action's `includeUpdaterJson` handle it (it aggregates per-platform artifacts uploaded by the action itself), or keep the custom validation path — but the action only includes platforms whose artifacts it uploaded itself. The existing `release.yml` design (action uploads mac/Windows, custom job builds latest.json for all) requires the custom job to also cover mac/Windows entries, which `check-release-assets.py` appears designed to do.

### 3.8 Platform specifics

- **Linux**: only the **AppImage** is updatable. The `.deb` cannot auto-update (Tauri ships no apt repository mechanism; the updater only knows AppImage for Linux — the updater bundle for Linux is the AppImage itself, https://v2.tauri.app/plugin/updater/#building). Users on `.deb` installs will simply never receive updater bundles. The repo bundles both `deb` and `appimage` targets.
- **Windows**: both MSI and NSIS installers are supported updater bundles; `installMode` (`passive` default / `basicUi` / `quiet`) controls installer UI (https://v2.tauri.app/plugin/updater/#installmode-on-windows). `quiet` cannot request admin privileges. The app exits automatically when the installer is launched (see 3.4).
- **macOS**: the updater bundle is `myapp.app.tar.gz` which replaces the `.app`. Current official docs state **no explicit code-signing requirement for the updater itself**, but distribution reality applies: an app downloaded from the internet must be signed to avoid Gatekeeper breakage. This repo configures **ad-hoc signing** (`"signingIdentity": "-"`, `tauri.conf.json:122`). Per https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing, ad-hoc signing "does not prevent MacOS from requiring users to whitelist the installation in their Privacy & Security settings", and per https://v2.tauri.app/distribute/pipelines/github/ ad-hoc signing Apple Silicon builds "can avoid macOS treating … builds downloaded from GitHub releases as damaged." For a seamless macOS update experience a Developer ID (+ notarization) certificate is the supported path (https://v2.tauri.app/distribute/sign/macos/#notarization: "Notarization is required when using a *Developer ID Application* certificate"). The CI secrets for `APPLE_CERTIFICATE` etc. already exist in `build.yml:673-679`.
- **Android/iOS**: updater plugin is not supported there (plugin README platform table) — irrelevant for this desktop app.

### 3.9 Other pitfalls for this version

- **No releases exist yet.** See §4 — the endpoint URL will 404 until a release containing `latest.json` is published.
- **Endpoint fallback semantics**: "Tauri will only continue to the next url if a non-2XX status code is returned" — a malformed manifest (200 with invalid JSON) aborts, it does not fall through.
- **Version comparison**: by default the update installs only if `update.version > current.version`; downgrades require a Rust-side `version_comparator` override (https://v2.tauri.app/plugin/updater/#allowing-downgrades).
- **CSP**: the updater HTTP traffic is native (reqwest inside the plugin), so the webview `connect-src` CSP in `tauri.conf.json` does not affect it.
- **`Update` resource leak**: if the user declines, call `update.close()` (JS API docs); the dialog flow should not `check()` repeatedly without closing — `UpdateDialog.tsx` currently calls `check()` up to twice per flow.
- **MSRV float** (see §1): commit `Cargo.lock` or pin `tauri-plugin-updater` before enabling, so CI builds reproducibly.

## 4. GitHub Releases state (fetched 2026-11-17)

- `https://github.com/timurmazitov/samobranka/releases` → **"There aren't any releases here"**. No release tags exist.
- Consequently there is **no `latest.json` asset anywhere, no updater bundles (`.sig`, `.app.tar.gz`, etc.), and no latest tag**. `https://github.com/timurmazitov/samobranka/releases/latest/download/latest.json` currently 404s.
- The release pipeline itself is ready in-repo: `.github/workflows/release.yml` (manual `workflow_dispatch`) creates a **draft** release, builds all platforms via reusable `.github/workflows/build.yml` (`tauri-apps/tauri-action@v0`, `projectPath: frontend`, signing env vars wired), validates assets, and uploads `latest.json` only when `plugins.updater.pubkey` is present in `tauri.conf.json` (release.yml:119-172). Platform workflows: `build.yml`, `build-windows.yml`, `build-linux.yml`, `build-devtest.yml`, `build-test.yml`, `pr-main-check.yml`. All triggers are manual (`WORKFLOWS_OVERVIEW.md`).

**Implication:** even with perfect client config, auto-update cannot work until (a) a minisign key pair is generated, (b) `pubkey` + `endpoints` + `createUpdaterArtifacts: true` + `updater:default` permission + plugin registration are restored, and (c) a release is built and **published** (not left as draft) with `latest.json` and signed artifacts attached.

## 5. Minimal re-enable checklist (derived from the above)

1. `pnpm tauri signer generate -w ~/.tauri/samobranka.key`; store the private key as the `TAURI_SIGNING_PRIVATE_KEY` repo secret (password → `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; both already referenced by the workflows).
2. `frontend/src-tauri/tauri.conf.json`: set `bundle.createUpdaterArtifacts: true`; add `plugins.updater` with `pubkey` (key content, not path) and `endpoints: ["https://github.com/timurmazitov/samobranka/releases/latest/download/latest.json"]`.
3. Add `"updater:default"` to the inline capability permissions in the same file.
4. `frontend/src-tauri/src/lib.rs`: restore `.plugin(tauri_plugin_updater::Builder::new().build())` (lib.rs:578).
5. Optionally clean up `updateService.ts` (`update?.available` → null check; call `update.close()` on decline).
6. Run the manual `Release` workflow, review the draft, publish it. Verify `latest.json` is an asset and that `…/releases/latest/download/latest.json` returns the manifest.
7. Optionally pin/commit `Cargo.lock` to stop the plugin version (and its MSRV) from floating.

## Sources

- Updater plugin guide: https://v2.tauri.app/plugin/updater/
- Updater JS API reference: https://v2.tauri.app/reference/javascript/updater/
- Updater plugin README (Rust registration, platform support, MSRV): https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/updater
- `tauri-plugin-updater 2.3.0` `Config` struct: https://docs.rs/tauri-plugin-updater/2.3.0/tauri_plugin_updater/struct.Config.html
- CLI reference (`signer generate`, `signer sign`, env vars): https://v2.tauri.app/reference/cli/
- tauri-action v0 `action.yml` (input names incl. `includeUpdaterJson`): https://github.com/tauri-apps/tauri-action/blob/v0/action.yml
- tauri-action v1 README (renamed `uploadUpdaterJson`, `releaseId`-without-`tagName` caveat): https://github.com/tauri-apps/tauri-action#readme
- GitHub pipeline guide: https://v2.tauri.app/distribute/pipelines/github/
- macOS signing / ad-hoc / notarization: https://v2.tauri.app/distribute/sign/macos/
- Repo facts: `frontend/src-tauri/Cargo.toml`, `frontend/src-tauri/tauri.conf.json`, `frontend/src-tauri/src/lib.rs`, `frontend/src/services/updateService.ts`, `frontend/src/components/UpdateDialog.tsx`, `frontend/src/components/About.tsx`, `.github/workflows/{release,build}.yml`, `docs/adr/0002-rebrand-to-samobranka.md`, `https://github.com/timurmazitov/samobranka/releases`.
