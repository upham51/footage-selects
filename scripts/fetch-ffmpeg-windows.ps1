# Download a static LGPL ffmpeg and ffprobe for 64-bit Windows from
# BtbN/FFmpeg-Builds, verify the checksum, and place them where the Tauri
# bundler expects sidecar binaries. Also copies them to dev-bin\ for the tests.
# Needs the GitHub CLI (present on GitHub's runners) and GH_TOKEN.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$triple = "x86_64-pc-windows-msvc"
$repo = "BtbN/FFmpeg-Builds"
New-Item -ItemType Directory -Force -Path src-tauri\binaries, dev-bin | Out-Null
$tmp = Join-Path $env:RUNNER_TEMP "ffmpeg-dl"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

# Prefer the newest release-branch build (for example ffmpeg-n8.0-latest-win64-lgpl-8.0.zip),
# falling back to the master build if the naming ever changes.
$names = gh release view latest --repo $repo --json assets -q ".assets[].name"
$asset = $names | Where-Object { $_ -match '^ffmpeg-n[0-9.]+-latest-win64-lgpl-[0-9.]+\.zip$' } |
  Sort-Object { [version](($_ -replace '^ffmpeg-n([0-9.]+)-.*$', '$1') + '.0') } | Select-Object -Last 1
if (-not $asset) { $asset = "ffmpeg-master-latest-win64-lgpl.zip" }
Write-Host "Using $asset"
gh release download latest --repo $repo --pattern $asset --pattern "checksums.sha256" --dir $tmp --clobber

$expected = (Get-Content (Join-Path $tmp "checksums.sha256") | Where-Object { $_ -match [regex]::Escape($asset) + '$' }) -split '\s+' | Select-Object -First 1
$actual = (Get-FileHash (Join-Path $tmp $asset) -Algorithm SHA256).Hash.ToLower()
if (-not $expected -or $expected.ToLower() -ne $actual) { throw "Checksum mismatch for ${asset}: expected $expected, got $actual" }
Write-Host "Checksum OK $actual"

Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath (Join-Path $tmp "x") -Force
$bin = Get-ChildItem -Path (Join-Path $tmp "x") -Recurse -Filter ffmpeg.exe | Select-Object -First 1
$dir = $bin.DirectoryName
Copy-Item (Join-Path $dir "ffmpeg.exe") "src-tauri\binaries\ffmpeg-$triple.exe" -Force
Copy-Item (Join-Path $dir "ffprobe.exe") "src-tauri\binaries\ffprobe-$triple.exe" -Force
Copy-Item (Join-Path $dir "ffmpeg.exe"), (Join-Path $dir "ffprobe.exe") dev-bin -Force
& dev-bin\ffmpeg.exe -hide_banner -version | Select-Object -First 1
& dev-bin\ffmpeg.exe -hide_banner -hwaccels
