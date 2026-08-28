[CmdletBinding()]
param(
    [string]$Version = $(if ($env:OPENWIFIDIAG_VERSION) { $env:OPENWIFIDIAG_VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:OPENWIFIDIAG_INSTALL_DIR) { $env:OPENWIFIDIAG_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\openwifidiag" }),
    # Install this local openwifidiag.exe instead of downloading a release.
    [string]$Binary = $(if ($env:OPENWIFIDIAG_BINARY) { $env:OPENWIFIDIAG_BINARY } else { "" }),
    # Build and install from the local source checkout (requires cargo).
    [switch]$FromSource
)
$ErrorActionPreference = "Stop"

if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne "X64") {
    throw "Only x64 Windows is currently available as a prebuilt release."
}
$platform = "win32-x64"
$base = "https://github.com/SunnySkye/openwifidiag/releases"
$url = if ($Version -eq "latest") { "$base/latest/download/openwifidiag-$platform.exe" } else { "$base/download/$Version/openwifidiag-$platform.exe" }

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$destination = Join-Path $InstallDir "openwifidiag.exe"
$sourceNote = $null

if ($Binary) {
    if (-not (Test-Path $Binary)) { throw "The -Binary option does not exist: $Binary" }
    Copy-Item -Force $Binary $destination
    $sourceNote = "Installed from the supplied local binary."
} elseif ($FromSource) {
    $sourceDir = Split-Path -Parent $PSScriptRoot
    if (-not (Test-Path (Join-Path $sourceDir "Cargo.toml"))) {
        throw "The -FromSource option requires a source checkout next to this installer."
    }
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) {
        $cargo = Get-Item "$env:USERPROFILE\.cargo\bin\cargo.exe"
    }
    if (-not $cargo) {
        throw "The -FromSource option requires cargo; install Rust from https://rustup.rs and retry."
    }
    Write-Host "Building release binary from $sourceDir ..."
    & $cargo.Source build --release --locked --manifest-path (Join-Path $sourceDir "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "The local Rust build failed." }
    Copy-Item -Force (Join-Path $sourceDir "target\release\openwifidiag.exe") $destination
    $sourceNote = "Built and installed from the local source checkout."
} else {
    $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("openwifidiag-" + [guid]::NewGuid() + ".exe")
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $temporary
        Move-Item -Force $temporary $destination
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue $temporary
    }
    $sourceNote = "Installed from a prebuilt GitHub Release."
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ";" | Where-Object { $_ })
if ($InstallDir -notin $parts) {
    [Environment]::SetEnvironmentVariable("Path", (($parts + $InstallDir) -join ";"), "User")
    $env:Path += ";$InstallDir"
    Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use it."
}
Write-Host "Installed $(& $destination --version 2>$null) to $destination"
Write-Host $sourceNote
