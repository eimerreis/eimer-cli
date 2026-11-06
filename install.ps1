# eimer installer for Windows.
#
#   irm https://raw.githubusercontent.com/eimerreis/eimer-cli/main/install.ps1 | iex
#
# Environment variables:
#   EIMER_VERSION      install a specific version (e.g. 0.2.0) instead of the latest
#   EIMER_INSTALL_DIR  install directory (default: %LOCALAPPDATA%\eimer\bin)
#   EIMER_NO_LINKS=1   do not create the task/pr/pipeline/release shims
#   GITHUB_TOKEN       optional, avoids GitHub API rate limits

$ErrorActionPreference = "Stop"

$Repo = "eimerreis/eimer-cli"
$TagPrefix = "eimer/v"
$InstallDir = if ($env:EIMER_INSTALL_DIR) { $env:EIMER_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "eimer\bin" }

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    default { throw "Unsupported architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}
$asset = "eimer-$arch-pc-windows-msvc.exe"

$headers = @{ Accept = "application/vnd.github+json" }
if ($env:GITHUB_TOKEN) { $headers.Authorization = "Bearer $($env:GITHUB_TOKEN)" }

$version = "$($env:EIMER_VERSION)".TrimStart("v")
if (-not $version) {
    # Newest stable eimer release; release/v* tags belong to @tapio/release.
    $releases = Invoke-RestMethod -Headers $headers "https://api.github.com/repos/$Repo/releases?per_page=50"
    $version = $releases |
        Where-Object { -not $_.draft -and -not $_.prerelease -and $_.tag_name -match '^eimer/v\d+\.\d+\.\d+$' } |
        ForEach-Object { [version]($_.tag_name.Substring($TagPrefix.Length)) } |
        Sort-Object -Descending |
        Select-Object -First 1
    if (-not $version) { throw "Could not determine the latest eimer release of $Repo." }
    $version = $version.ToString()
}

$base = "https://github.com/$Repo/releases/download/$TagPrefix$version"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Write-Host "i Downloading eimer $version ($asset)"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile "$tmp\eimer.exe"
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset.sha256" -OutFile "$tmp\eimer.sha256"

    $expected = ((Get-Content "$tmp\eimer.sha256" -Raw).Trim() -split '\s+')[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 "$tmp\eimer.exe").Hash.ToLower()
    if ($expected -ne $actual) { throw "Checksum mismatch (expected $expected, got $actual)." }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Move-Item -Force "$tmp\eimer.exe" (Join-Path $InstallDir "eimer.exe")
    Write-Host "+ Installed eimer $version to $InstallDir\eimer.exe"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

if ($env:EIMER_NO_LINKS -ne "1") {
    & (Join-Path $InstallDir "eimer.exe") link --dir $InstallDir
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not (($userPath -split ";") -contains $InstallDir)) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
    Write-Host "+ Added $InstallDir to your user PATH (restart your terminal)."
}

if (-not (Get-Command az -ErrorAction SilentlyContinue)) {
    Write-Warning "Azure CLI (az) not found. eimer needs it for Azure DevOps: https://learn.microsoft.com/cli/azure/install-azure-cli"
}
Write-Host "i Update later with: eimer upgrade"
