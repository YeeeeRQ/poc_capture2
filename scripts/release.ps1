[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$SkipClean
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Get-CargoVersion {
    $guiCargo = Join-Path $ProjectRoot "capture-gui\Cargo.toml"
    $content = Get-Content $guiCargo -Raw
    $match = [regex]::Match($content, 'version\s*=\s*"([^"]+)"')
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    throw "Failed to parse version from capture-gui/Cargo.toml"
}

function Test-Binaries {
    $ffmpeg = Join-Path $ProjectRoot "bin\ffmpeg.exe"
    $ffprobe = Join-Path $ProjectRoot "bin\ffprobe.exe"
    if (-not (Test-Path $ffmpeg)) {
        throw "FFmpeg not found: $ffmpeg"
    }
    if (-not (Test-Path $ffprobe)) {
        throw "FFprobe not found: $ffprobe"
    }
    Write-Host "[OK] FFmpeg and FFprobe found"
}

function Build-Release {
    Write-Host "=== Building release ==="
    Push-Location $ProjectRoot
    try {
        if (-not $SkipClean) {
            cargo clean -p capture-gui -p capture-cli
        }
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Build failed with exit code $LASTEXITCODE"
        }
        Write-Host "[OK] Build successful"
    }
    finally {
        Pop-Location
    }
}

function New-StagingDirectory {
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $stagingDir = Join-Path $ProjectRoot "target\staging_$timestamp"
    New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null
    return $stagingDir
}

function Copy-ReleaseBinaries {
    param([string]$StagingDir)

    Write-Host "=== Copying binaries ==="

    $guiExe = Join-Path $ProjectRoot "target\release\capture-gui.exe"
    $cliExe = Join-Path $ProjectRoot "target\release\capture-cli.exe"

    if (-not (Test-Path $guiExe)) {
        throw "capture-gui.exe not found: $guiExe"
    }
    if (-not (Test-Path $cliExe)) {
        throw "capture-cli.exe not found: $cliExe"
    }

    Copy-Item $guiExe $StagingDir -Force
    Copy-Item $cliExe $StagingDir -Force
    Write-Host "[OK] Copied capture-gui.exe and capture-cli.exe"
}

function Copy-Ffmpeg {
    param([string]$StagingDir)

    Write-Host "=== Copying FFmpeg ==="

    $ffmpeg = Join-Path $ProjectRoot "bin\ffmpeg.exe"
    $ffprobe = Join-Path $ProjectRoot "bin\ffprobe.exe"

    Copy-Item $ffmpeg $StagingDir -Force
    Copy-Item $ffprobe $StagingDir -Force
    Write-Host "[OK] Copied FFmpeg and FFprobe"
}

function New-Readme {
    param([string]$Version, [string]$StagingDir)

    Write-Host "=== Generating README ==="

    $readme = @"
capture $Version
=============

Requirements:
- Windows 10 version 1903 or later
- Windows 11

Usage:
  capture-gui.exe    GUI application for screen capture
  capture-cli.exe    Command-line interface

FFmpeg (bundled):
  ffmpeg.exe         Video encoder
  ffprobe.exe        Media diagnostics tool

Notes:
- FFmpeg is required for video encoding (MP4)
- If FFmpeg is not found, raw RGBA fallback mode will be used
- Screenshots are saved as PNG files

"@

    $readmePath = Join-Path $StagingDir "README.txt"
    $readme | Out-File -FilePath $readmePath -Encoding UTF8
    Write-Host "[OK] Created README.txt"
}

function Compress-Package {
    param([string]$Version, [string]$StagingDir)

    Write-Host "=== Creating package ==="

    $zipName = "capture-$Version-win64.zip"
    $zipPath = Join-Path $ProjectRoot "target\$zipName"

    Set-Location $StagingDir
    try {
        Compress-Archive -Path @(
            "capture-gui.exe",
            "capture-cli.exe",
            "ffmpeg.exe",
            "ffprobe.exe",
            "README.txt"
        ) -DestinationPath $zipPath -Force
    }
    finally {
        Set-Location $ProjectRoot
    }

    $size = (Get-Item $zipPath).Length / 1MB
    Write-Host "[OK] Package created: $zipName ($([math]::Round($size, 1)) MB)"
}

function Remove-StagingDirectory {
    param([string]$StagingDir)

    if (Test-Path $StagingDir) {
        Remove-Item $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$ProjectRoot = "D:\Project4Rust\poc_capture2"

Write-Host "========================================"
Write-Host "  capture Release Script"
Write-Host "========================================"
Write-Host ""

try {
    $version = Get-CargoVersion
    Write-Host "Version: $version"
    Write-Host ""

    Test-Binaries

    if (-not $SkipBuild) {
        Build-Release
    } else {
        Write-Host "[SKIP] Build skipped"
    }

    $stagingDir = New-StagingDirectory

    Copy-ReleaseBinaries -StagingDir $stagingDir
    Copy-Ffmpeg -StagingDir $stagingDir
    New-Readme -Version $version -StagingDir $stagingDir
    Compress-Package -Version $version -StagingDir $stagingDir

    Remove-StagingDirectory -StagingDir $stagingDir

    Write-Host ""
    Write-Host "========================================"
    Write-Host "  Release completed successfully!"
    Write-Host "========================================"
}
catch {
    Write-Host ""
    Write-Host "[ERROR] $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
