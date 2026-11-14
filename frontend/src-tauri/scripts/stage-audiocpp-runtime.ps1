#Requires -Version 5.1
<#
.SYNOPSIS
  Stages the audio.cpp native runtime for Windows bundling.

.DESCRIPTION
  Downloads the pinned audio.cpp release lib archive (hash-verified),
  then copies the audioCpp runtime into the Tauri staging directory:

    - audiocpp.dll                 (the native engine, C ABI)
    - msvcp140*.dll                (MSVC C++ runtime)
    - vcruntime140*.dll            (MSVC C runtime)
    - vcomp140.dll                 (MSVC OpenMP runtime)
    - audio.cpp-LICENSE.txt        (Apache-2.0 license of the engine)

  Staging follows the existing sidecar pattern: files land in
  frontend/src-tauri/audiocpp-runtime/, which tauri.windows.conf.json
  maps into the install root next to the executable. The app discovers
  audiocpp.dll next to the executable (audio_cpp_engine/wrapper.rs),
  and the Windows loader resolves the bundled MSVC runtime DLLs from
  the same directory, so the installed app runs on a machine without
  the Visual C++ Redistributable.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File stage-audiocpp-runtime.ps1 `
    -Version "v0.9.4" `
    -Archive "audio-v0.9.4-lib-windows-x64-vulkan.zip" `
    -Sha256 "cf2fabd8630050491a92c002d7bdfd29eca722c31fb3b8cb0ba9b9c5e6e9ff3d" `
    -Destination "frontend/src-tauri/audiocpp-runtime"
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$Archive,

  [Parameter(Mandatory = $true)]
  [string]$Sha256,

  # Path to the staging directory, relative to the repository root.
  [Parameter(Mandatory = $true)]
  [string]$Destination
)

$ErrorActionPreference = 'Stop'

# scripts/ -> src-tauri/ -> frontend/ -> repository root
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$stageDir = Join-Path $repoRoot $Destination
$url = "https://github.com/0xShug0/audio.cpp/releases/download/$Version/$Archive"

Write-Host "Downloading audio.cpp $Version runtime..."
Write-Host "  $url"

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("audiocpp-stage-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
  $archivePath = Join-Path $tempDir $Archive
  Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing

  $actualHash = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
  $expectedHash = $Sha256.ToLowerInvariant()
  if ($actualHash -ne $expectedHash) {
    Write-Error "SHA256 mismatch for $Archive`n  expected: $expectedHash`n  actual:   $actualHash"
    exit 1
  }
  Write-Host "SHA256 verified: $actualHash"

  $extractDir = Join-Path $tempDir "extract"
  Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

  $libsDir = Join-Path $extractDir "libs"
  $engineDll = Join-Path $libsDir "audiocpp.dll"
  $licenseFile = Join-Path $extractDir "LICENSE"
  if (!(Test-Path $engineDll)) {
    Write-Error "Archive does not contain libs/audiocpp.dll"
    exit 1
  }
  if (!(Test-Path $licenseFile)) {
    Write-Error "Archive does not contain LICENSE"
    exit 1
  }

  New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

  Copy-Item -Path $engineDll -Destination $stageDir
  Get-ChildItem -Path $libsDir -Filter "msvcp140*.dll" | Copy-Item -Destination $stageDir
  Get-ChildItem -Path $libsDir -Filter "vcruntime140*.dll" | Copy-Item -Destination $stageDir
  Copy-Item -Path (Join-Path $libsDir "vcomp140.dll") -Destination $stageDir
  Copy-Item -Path $licenseFile -Destination (Join-Path $stageDir "audio.cpp-LICENSE.txt")

  Write-Host "Staged audio.cpp $Version runtime to $stageDir :"
  Get-ChildItem -Path $stageDir | ForEach-Object { Write-Host "  $($_.Name) ($($_.Length) bytes)" }
} finally {
  Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}
