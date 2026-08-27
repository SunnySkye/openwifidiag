[CmdletBinding()]
param(
    [string]$Version = $(if ($env:OPENWIFIDIAG_VERSION) { $env:OPENWIFIDIAG_VERSION } else { "latest" }),
    [string]$InstallDir = $(if ($env:OPENWIFIDIAG_INSTALL_DIR) { $env:OPENWIFIDIAG_INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs\openwifidiag" })
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
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("openwifidiag-" + [guid]::NewGuid() + ".exe")
try {
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $temporary
    Move-Item -Force $temporary $destination
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $temporary
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ";" | Where-Object { $_ })
if ($InstallDir -notin $parts) {
    [Environment]::SetEnvironmentVariable("Path", (($parts + $InstallDir) -join ";"), "User")
    $env:Path += ";$InstallDir"
    Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use it."
}
Write-Host "Installed openwifidiag to $destination"
