## 🐧 Building on Linux

Practical guide to building Samobranka on Linux. For the step-by-step developer
walkthrough (system packages, sidecar, native runtime staging, troubleshooting)
see [RUNNING_LINUX.md](RUNNING_LINUX.md); for cross-platform build notes see
[BUILDING.md](BUILDING.md).

There are no compile-time GPU build variants. The app compiles CPU-only; the
single transcription engine (transcribe.cpp) decides its compute mode at
runtime (Auto / Vulkan / CPU) and uses the system Vulkan loader from the GPU
drivers. See the [GPU acceleration guide](GPU_ACCELERATION.md).

---

## Quick Start

### 1. Install Basic Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git

# Arch Linux
sudo pacman -S base-devel cmake git
```

### 2. Prepare the Native Pieces

From the repository root, after the [first-run sidecar setup](BUILDING.md#first-run-sidecar-setup):

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm run stage:transcribe
```

### 3. Build and Run

```bash
# Development mode (with hot reload)
pnpm run tauri:dev

# Production build
pnpm run tauri:build
```

### Build Output Location

After a successful build:

```
target/release/bundle/appimage/Samobranka_<version>_amd64.AppImage
target/release/bundle/deb/Samobranka_<version>_amd64.deb
```

---

## 📦 Release packages and updates

Release builds are produced by `.github/workflows/release.yml`, which builds `deb` and
`appimage` packages for Linux alongside the Windows installers in one draft release.
macOS is not built: CI has no Apple Developer certificate.

Linux packages are signed with minisign: `tauri signer sign` writes a `.deb.sig` or
`.AppImage.sig` file next to each package. The AppImage signature doubles as the updater
signature. A local `tauri build` does not sign anything.

**Automatic updates are enabled.** The app fetches `latest.json` from this repository's
GitHub Releases; the release pipeline verifies the package signatures with the minisign
public key committed in `frontend/src-tauri/tauri.conf.json` and uploads the manifest to
the release. Only the AppImage auto-updates on Linux (the `.deb` never will), and the
manifest covers Windows and Linux only — macOS is not built.

---

## 🎙️ Audio device names (PipeWire/PulseAudio)

### System audio requirements

Linux system-audio capture requires a running PulseAudio-compatible server:
either PulseAudio or PipeWire with its PulseAudio compatibility service
(`pipewire-pulse`, called `pipewire-pulseaudio` on Fedora). PipeWire alone is
insufficient. Users already running PulseAudio do not need to install PipeWire.
Minimal or customized installations may omit the compatibility service; see the
[Fedora documentation](https://fedoraproject.org/wiki/Changes/DefaultPipeWire) and
[Debian documentation](https://wiki.debian.org/PipeWire).

If the server cannot be reached, system audio is unavailable. Ensure PulseAudio
or PipeWire’s PulseAudio compatibility service is running.

DEB packages declare `libpulse0` and `libasound2`, alongside `libvulkan1`, as
runtime dependencies. These client libraries do not install or select an audio
server. `libvulkan1` backs the runtime Vulkan path of the transcribe.cpp
engine; without a usable driver the engine falls back to CPU.

On Linux, the **Microphone** and **System Audio** pickers are populated directly
from PulseAudio/PipeWire introspection. The labels you see in Samobranka are the
same ones shown by the desktop's native audio settings (KDE, GNOME, etc.).

- **Microphone** lists real input sources (`alsa_input.*`) with their human-readable
  descriptions. Sink monitors are excluded because they appear under **System Audio**.
- **System Audio** lists playback sinks and captures through each sink's monitor
  source, also using the server's own descriptions.

The available entries depend on the **active card profile**, exactly as in the
system settings: a USB headset set to output-only profile will not show up as a
microphone.

If no PulseAudio/`pipewire-pulse` server is reachable, Samobranka falls back to a
small whitelist of ALSA microphone endpoints (`default`, `pipewire`, `pulse`).
This fallback does not provide system-audio capture.

If a PulseAudio source or sink list callback fails, Samobranka rejects the entire
inventory rather than using an empty or partial list as evidence of device loss
or a reason to select ALSA fallback. The device monitor retains its previous
snapshot until a successful poll.

### Recording device selection

Default Microphone and Default System Audio follow changes to the OS default
while recording, checked approximately every two seconds. Explicit device
selections stay pinned until unavailable. An unavailable microphone port is
treated as device loss; unknown port availability remains usable. Automatic
fallback preserves the saved selection and stays fixed for that meeting.
Switching between speakers and headphones on the same output device does not
require restarting system-audio capture.

---

## 🧭 Troubleshooting

Build and runtime problems are collected in
[RUNNING_LINUX.md](RUNNING_LINUX.md#8-частые-проблемы), including the transcribe.cpp
runtime staging and the CPU fallback warning.

---

**Need help?** Open an issue on GitHub with your GPU type, distro, and the relevant
build output.
