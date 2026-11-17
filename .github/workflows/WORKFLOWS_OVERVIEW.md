# GitHub Actions Workflows Overview

This document provides a quick overview of all available CI/CD workflows in this repository.

**Note:** All workflows in this repository use **manual triggers only** (`workflow_dispatch`). There are no automatic triggers from push or pull request events.

## Workflow Files

### 1. **build-devtest.yml** - DevTest Builds
**Purpose:** Fast builds for development and testing

**Key Features:**
- Signing OFF by default (faster builds)
- Optional signing via workflow dispatch input
- All platforms in parallel
- 14-day artifact retention

**Triggers:**
- Manual dispatch only

**Use When:**
- Regular development work
- Testing features
- Need fast feedback

---

### 2. **build-windows.yml** - Windows Standalone Builds
**Purpose:** Build and test specifically for Windows x64

**Key Features:**
- DigiCert KeyLocker signing (cloud HSM)
- Signs both MSI and NSIS installers
- Signature verification with PowerShell
- MSI installer validation

**Triggers:**
- Manual dispatch only

**Use When:**
- Windows-specific development
- Testing Vulkan GPU acceleration
- Verifying Windows-specific features

**Outputs:**
- `.msi` installer
- `.exe` NSIS installer

---

### 3. **build-linux.yml** - Linux Standalone Builds
**Purpose:** Build and test for Linux distributions

**Key Features:**
- Support for Ubuntu 22.04 and 24.04
- Multiple bundle formats (DEB, AppImage, RPM)
- No package signing (release signing lives in `build.yml`)
- AppImage compatibility fixes
- Package verification

**Triggers:**
- Manual dispatch only

**Use When:**
- Linux-specific development
- Testing Vulkan GPU acceleration
- Verifying package formats

**Outputs:**
- `.deb` package (Ubuntu/Debian)
- `.AppImage` portable
- `.rpm` package (Fedora/RHEL)

---

### 4. **build-test.yml** - Multi-Platform Test Builds
**Purpose:** Test builds across all platforms with signing

**Key Features:**
- Signing ON by default
- All platforms in parallel
- Uses reusable `build.yml` workflow
- 30-day artifact retention
- Artifacts prefixed with `samobranka-test-`

**Triggers:**
- Manual dispatch only

**Use When:**
- Pre-release testing
- Verifying signing infrastructure
- Testing across all platforms simultaneously

---

### 5. **build.yml** - Reusable Build Workflow
**Purpose:** Shared workflow used by other workflows

**Key Features:**
- Reusable workflow (called by others)
- Highly configurable inputs
- Used by `build-test.yml` and `release.yml`

**Not directly triggered** - used as a building block

---

### 6. **release.yml** - Production Release
**Purpose:** Create official releases with signed binaries

**Key Features:**
- Signing REQUIRED
- Creates GitHub Release (draft)
- Version tags from `tauri.conf.json`
- Uploads release assets
- Builds Linux (`deb`, `appimage`) and Windows; macOS is not built (no Apple
  Developer certificate in CI)
- Fails if tag `v<version>` already exists — bump the version in `tauri.conf.json` instead
- Generates the updater manifest: signatures are verified and `latest.json` is
  uploaded to the release

**Triggers:**
- Manual dispatch only

**Use When:**
- Ready to publish a new version
- Creating official release artifacts

**Outputs:**
- GitHub Release (draft)
- Windows: MSI installer (signed via `signCommand`), NSIS installer
- Linux: DEB and AppImage packages, each with a minisign `.sig`
- `latest.json` updater manifest (Windows + Linux targets)
- Release notes auto-generated

**Version Behavior:**
- If tag `v<version>` exists, the workflow exits with an error; raise the version in
  `frontend/src-tauri/tauri.conf.json` and release again

---

### 7. **pr-main-check.yml** - Validation Check
**Purpose:** Quick validation of version and configuration

**Key Features:**
- No builds triggered
- Validates version format
- Shows current branch info
- Provides next steps guidance

**Triggers:**
- Manual dispatch only

**Use When:**
- Quick configuration check
- Before running full builds

---

## How to Run Workflows

1. **Go to Actions tab** in GitHub repository
2. **Select workflow** from left sidebar
3. **Click "Run workflow"** button
4. **Select branch** to run against
5. **Configure options** (build type, signing, etc.)
6. **Click "Run workflow"** to start
7. **Monitor progress** in the Actions tab

---

## Quick Decision Guide

### "I'm developing a new feature..."
- **Use `build-devtest.yml`** (manual dispatch)
- Fast builds, no signing by default
- Enable signing checkbox if needed

### "I need to test Windows-specific code..."
- **Use `build-windows.yml`** (manual dispatch)
- Focus on Windows
- Optional signing

### "I need to test Linux packages..."
- **Use `build-linux.yml`** (manual dispatch)
- Choose Ubuntu version
- Choose bundle types

### "I need signed builds for all platforms..."
- **Use `build-test.yml`** (manual dispatch)
- All platforms
- Signing enabled
- Full verification

### "I'm ready to release..."
- **Use `release.yml`** (manual dispatch)
- Creates GitHub Release
- All platforms, fully signed
- Production-ready artifacts

---

## Workflow Dependencies

```
build.yml (reusable)
    |-- build-test.yml (calls build.yml)
    |-- release.yml (calls build.yml)

Standalone (don't use build.yml):
    |-- build-windows.yml
    |-- build-linux.yml
    |-- build-devtest.yml
    |-- pr-main-check.yml (validation only)
```

---

## Comparison Matrix

| Workflow | Platforms | Default Signing | Speed | Retention | Use Case |
|----------|-----------|----------------|-------|-----------|----------|
| `build-devtest.yml` | All | OFF | Fast | 14 days | Development |
| `build-windows.yml` | Windows | Optional | Medium | 30 days | Windows dev |
| `build-linux.yml` | Linux | Optional | Medium | 30 days | Linux dev |
| `build-test.yml` | All | ON | Slow | 30 days | Pre-release |
| `release.yml` | Windows + Linux | REQUIRED | Slow | Permanent | Release |

---

## Artifact Naming Convention

```
samobranka-{workflow}-{platform}-{target}-{version}
```

**Examples:**
- `samobranka-devtest-macOS-aarch64-apple-darwin-0.1.3`
- `samobranka-test-windows-x86_64-pc-windows-msvc-0.1.3`
- `samobranka-macos-aarch64-release-0.1.3`

---

## Required Secrets

All workflows require these secrets to be configured:

### Windows Signing (DigiCert)
- `SM_HOST` - DigiCert host URL
- `SM_API_KEY` - API key
- `SM_CLIENT_CERT_FILE_B64` - Client cert (base64)
- `SM_CLIENT_CERT_PASSWORD` - Client cert password
- `SM_CODE_SIGNING_CERT_SHA1_HASH` - Certificate hash

### Tauri Updater (Windows + Linux)

> **NOTE:** The updater is enabled (endpoint: this repository's GitHub
> Releases). The generated `latest.json` covers Windows and Linux only —
> macOS is not built by the release pipeline (no Apple Developer certificate
> in CI).

- `TAURI_SIGNING_PRIVATE_KEY` - Ed25519 private key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Key password

### Application Configuration
<!-- TODO(brand): secrets/variables were renamed to the SAMOBRANKA_* prefix during the rebrand.
     Recreate them under the new names in the GitHub repo settings before the first samobranka release;
     old-prefixed entries can then be deleted. See docs/adr/0002-rebrand-to-samobranka.md. -->
- `SAMOBRANKA_RSA_PUBLIC_KEY` - License validation public key
- `SAMOBRANKA_DOODLE_URL` - Seasonal sidebar icon URL (repository variable, not a secret; unset = feature off)
- `SUPABASE_URL` - Online license verification
- `SUPABASE_ANON_KEY` - Supabase anonymous key

---

## Performance Tips

1. **Use devtest workflow** for routine development (fastest)
2. **Enable signing** only when necessary (adds 10-15 minutes)
3. **Test specific platforms** when working on platform-specific code
4. **Run full builds** (`build-test.yml`) before releases
5. **Cache is enabled** - subsequent builds are faster

---

## Troubleshooting

### Build fails with version error (Windows MSI)
- Ensure version in `tauri.conf.json` doesn't contain non-numeric pre-release identifiers
- Use `0.1.3` not `0.1.2-pro-trial`

### Signing fails
- Verify all required secrets are configured
- Check secret expiration dates
- Review workflow logs for specific errors

### Artifacts not available
- Check build succeeded completely
- Artifacts expire based on retention period
- Ensure `upload-artifacts` is enabled

### Workflow not appearing in Actions
- Verify YAML syntax is valid
- Check file is in `.github/workflows/` directory
- Ensure file extension is `.yml` or `.yaml`

---

## Support

For issues with workflows:
1. Check workflow logs in Actions tab
2. Review this documentation
3. Check `README_DEVTEST.md` for devtest-specific help
4. Check `ACCELERATION_GUIDE.md` for GPU/performance info
