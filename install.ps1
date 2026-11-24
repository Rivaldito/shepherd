# Shepherd installer for Windows: downloads the release zip (shepherd.exe and the ConPTY runtime beside it), checks
# its SHA-256 and installs it into %LOCALAPPDATA%\Shepherd\bin, which it adds to your user PATH.
#
#   irm https://raw.githubusercontent.com/Rivaldito/shepherd/main/install.ps1 | iex
#
# Windows gets the server, the cli and the terminal ui (`shepherd`); the desktop app is not built for Windows yet.
#
# Variables:
#   SHEPHERD_VERSION      release tag to install (default: the newest release, including betas), e.g. v0.4.0-beta
#   SHEPHERD_INSTALL_DIR  where to put the files (default: %LOCALAPPDATA%\Shepherd\bin)
#   SHEPHERD_RELEASE_BASE base URL holding the assets, for mirrors and tests (skips the GitHub lookup)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repo = "Rivaldito/shepherd"
$Asset = "shepherd-windows-x86_64.zip"
$InstallDir = if ($env:SHEPHERD_INSTALL_DIR) { $env:SHEPHERD_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Shepherd\bin" }

function Log($m) { Write-Host "==> $m" }

if (-not [Environment]::Is64BitOperatingSystem) { throw "Shepherd for Windows needs a 64-bit system." }
# Windows on ARM runs the x86_64 build through emulation

Write-Host ""
Write-Host "  shepherd installer"
Write-Host "  https://shepherd.rivaldito.com"
Write-Host ""

if ($env:SHEPHERD_RELEASE_BASE) {
    $Base = $env:SHEPHERD_RELEASE_BASE.TrimEnd("/")
    $Tag = "custom"
} else {
    $Tag = $env:SHEPHERD_VERSION
    if (-not $Tag) {
        Log "looking for the newest release..."
        # /releases (not /releases/latest): the latest release may be a beta, which /latest ignores
        $Tag = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases?per_page=1")[0].tag_name
        if (-not $Tag) { throw "no release found for $Repo" }
    }
    $Base = "https://github.com/$Repo/releases/download/$Tag"
}
Log "installing $Tag"

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("shepherd-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $Zip = Join-Path $Tmp $Asset
    try {
        Invoke-WebRequest "$Base/$Asset" -OutFile $Zip
        $Sums = (Invoke-WebRequest "$Base/$Asset.sha256").Content
        # release assets come as application/octet-stream, which PowerShell hands back as bytes
        if ($Sums -is [byte[]]) { $Sums = [Text.Encoding]::UTF8.GetString($Sums) }
    } catch {
        throw "could not download $Asset from $Base (does this release have a Windows build?): $($_.Exception.Message)"
    }
    $Want = ([regex]::Match([string]$Sums, '[0-9a-fA-F]{64}')).Value.ToLower()
    $Got = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower()
    if (-not $Want -or $Got -ne $Want) { throw "the checksum of $Asset does not match; not installing it" }

    # a running server keeps shepherd.exe locked: stop it first (its terminals and agents close)
    $Exe = Join-Path $InstallDir "shepherd.exe"
    if (Test-Path $Exe) {
        Log "stopping the running server, if any"
        try { & $Exe server stop *> $null } catch { }
        Start-Sleep -Seconds 1
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Expand-Archive -Path $Zip -DestinationPath $InstallDir -Force
    if (-not (Test-Path $Exe)) { throw "the archive does not contain shepherd.exe" }
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$Entries = @($UserPath -split ";" | Where-Object { $_ })
if ($Entries -notcontains $InstallDir) {
    [Environment]::SetEnvironmentVariable("Path", (($Entries + $InstallDir) -join ";"), "User")
    Log "added $InstallDir to your PATH (open a new terminal to use it)"
}

Write-Host ""
Log "next: start it in a git repository (the orchestrator is on by default)"
Write-Host "    cd C:\your-repo; shepherd"
Write-Host ""
