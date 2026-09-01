# txtify installer - PowerShell for Windows
# Usage:
#   irm https://raw.githubusercontent.com/example/txtify/main/installer/install.ps1 | iex
#   Invoke-WebRequest -Uri https://raw.githubusercontent.com/example/txtify/main/installer/install.ps1 -OutFile install.ps1; .\install.ps1 -Version v0.1.0 -Prefix "$env:LOCALAPPDATA\txtify"
# Security: always inspect script before piping to iex (Get-Content install.ps1 | More)
#
# Parameters:
#   -Version  Version to install (default: latest, e.g. v0.1.0)
#   -Prefix   Install directory (default: $env:LOCALAPPDATA\txtify, fallback $env:USERPROFILE\txtify)
#   -NoModifyPath  Do not modify PATH
#   -Force    Overwrite existing binary

[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$Prefix = "",
    [switch]$NoModifyPath,
    [switch]$Force,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$Repo = if ($env:TXTIFY_REPO) { $env:TXTIFY_REPO } else { "example/txtify" }
if ($env:TXTIFY_VERSION -and $Version -eq "latest") { $Version = $env:TXTIFY_VERSION }
if ($env:TXTIFY_PREFIX -and $Prefix -eq "") { $Prefix = $env:TXTIFY_PREFIX }

if ($Help) {
    @"
txtify installer (Windows PowerShell)

Usage: .\install.ps1 [-Version <v0.1.0>] [-Prefix <dir>] [-NoModifyPath] [-Force]

Parameters:
  -Version      Version to install (default: latest, e.g. v0.1.0)
  -Prefix       Install directory (default: `$env:LOCALAPPDATA\txtify)
  -NoModifyPath Do not modify PATH
  -Force        Overwrite existing binary
  -Help         Show this help

Environment:
  TXTIFY_VERSION, TXTIFY_REPO, TXTIFY_PREFIX

Examples:
  irm https://raw.githubusercontent.com/example/txtify/main/installer/install.ps1 | iex
  .\install.ps1 -Version v0.1.0
  .\install.ps1 -Prefix "`$env:USERPROFILE\bin"

Security:
  Inspect before piping: Get-Content install.ps1 | More
"@
    exit 0
}

# Execution policy hint
if ((Get-ExecutionPolicy) -eq "Restricted") {
    Write-Warning "ExecutionPolicy is Restricted. Run: Set-ExecutionPolicy RemoteSigned -Scope CurrentUser"
}

# Detect arch (only x86_64 for now; aarch64 will be txtify-aarch64-pc-windows-msvc)
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$Target = "${Arch}-pc-windows-msvc"
Write-Host "Detected target: $Target" -ForegroundColor Cyan

# Determine prefix
if (-not $Prefix) {
    $Prefix = Join-Path $env:LOCALAPPDATA "txtify"
    if (-not $Prefix) { $Prefix = Join-Path $env:USERPROFILE "txtify" }
}
Write-Host "Install prefix: $Prefix" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $Prefix | Out-Null

# Check existing
$BinPath = Join-Path $Prefix "txtify.exe"
if ((Test-Path $BinPath) -and (-not $Force)) {
    Write-Warning "Existing txtify found at $BinPath (use -Force to overwrite)"
}

# Resolve version -> URL
function Resolve-Url {
    param([string]$Ver, [string]$Tgt)
    if ($Ver -eq "latest") {
        return "https://github.com/$Repo/releases/latest/download/txtify-${Tgt}.zip"
    } else {
        if (-not $Ver.StartsWith("v")) { $Ver = "v$Ver" }
        return "https://github.com/$Repo/releases/download/$Ver/txtify-${Tgt}.zip"
    }
}
$Url = Resolve-Url -Ver $Version -Tgt $Target
$ShaUrl = "$Url.sha256"
Write-Host "Downloading $Url" -ForegroundColor Cyan

$TmpDir = Join-Path $env:TEMP "txtify-install-$(Get-Random)"
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null
$ZipPath = Join-Path $TmpDir "txtify.zip"
try {
    # Use TLS 1.2+
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    # Prefer curl.exe if available for better progress, else Invoke-WebRequest
    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
        $curlArgs = @("-fsSL", "--retry", "3", "--retry-delay", "2", "-o", $ZipPath, $Url)
        $proc = Start-Process -FilePath "curl.exe" -ArgumentList $curlArgs -NoNewWindow -PassThru -Wait
        if ($proc.ExitCode -ne 0) { throw "curl failed with exit $($proc.ExitCode)" }
    } else {
        Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
    }
} catch {
    Write-Error "Download failed: $Url`nTip: check https://github.com/$Repo/releases`n$_"
    exit 1
}

# Try checksum
try {
    $ShaPath = Join-Path $TmpDir "txtify.zip.sha256"
    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
        Start-Process -FilePath "curl.exe" -ArgumentList @("-fsSL", "-o", $ShaPath, $ShaUrl) -NoNewWindow -Wait | Out-Null
    } else {
        Invoke-WebRequest -Uri $ShaUrl -OutFile $ShaPath -UseBasicParsing -ErrorAction SilentlyContinue
    }
    if (Test-Path $ShaPath) {
        Write-Host "Verifying checksum..." -ForegroundColor Cyan
        $Expected = (Get-Content $ShaPath).Split(" ")[0].Trim().ToLower()
        $Actual = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()
        if ($Expected -ne $Actual) {
            Write-Error "Checksum mismatch! expected $Expected got $Actual"
            exit 1
        }
        Write-Host "Checksum OK" -ForegroundColor Green
    } else {
        Write-Host "No checksum file at $ShaUrl (skipping verification)" -ForegroundColor Yellow
    }
} catch {
    Write-Warning "Checksum verification skipped: $_"
}

Write-Host "Extracting..." -ForegroundColor Cyan
try {
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force
} catch {
    Write-Error "Failed to extract zip: $_"; exit 1
}

# Find binary
$BinSrc = Get-ChildItem -Path $TmpDir -Recurse -Filter "txtify.exe" | Select-Object -First 1
if (-not $BinSrc) {
    Write-Error "txtify.exe not found in archive"; Get-ChildItem -Recurse $TmpDir | Out-String | Write-Error; exit 1
}

# Install
try {
    Copy-Item -Path $BinSrc.FullName -Destination $BinPath -Force
    Write-Host "Installed txtify to $BinPath" -ForegroundColor Green
} catch {
    Write-Error "Failed to copy to $BinPath (try -Force or check permissions): $_"; exit 1
}

# Verify
try {
    & $BinPath --version
} catch {
    Write-Warning "Installed but --version failed: $_"
}

# Add to PATH (HKCU, no admin)
if (-not $NoModifyPath) {
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($CurrentPath -notlike "*$Prefix*") {
        Write-Host "Adding $Prefix to PATH (HKCU)..." -ForegroundColor Cyan
        $NewPath = if ($CurrentPath) { "$CurrentPath;$Prefix" } else { $Prefix }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:Path = "$env:Path;$Prefix"
        Write-Host "Added to PATH. Restart shell or run: `$env:Path += `";$Prefix`"" -ForegroundColor Yellow
    } else {
        Write-Host "PATH already contains $Prefix" -ForegroundColor Green
    }
}

# Cleanup
Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "Run 'txtify --help' and 'txtify doctor' to verify." -ForegroundColor Cyan
Write-Host "To add shell integration: txtify shell install" -ForegroundColor Cyan
Write-Host "To uninstall: txtify shell uninstall; Remove-Item -Recurse -Force `"$Prefix`""
