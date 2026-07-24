# SoulSystem installer for Windows PowerShell.
#
# irm https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repository = "Memorithm/SoulSystem"
$Binary = "soulsystem.exe"
$Version = if ($env:SOULSYSTEM_VERSION) { $env:SOULSYSTEM_VERSION } else { "latest" }
$InstallDirectory = if ($env:SOULSYSTEM_INSTALL_DIR) {
    $env:SOULSYSTEM_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "SoulSystem\bin"
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "SoulSystem requires 64-bit Windows."
}

$Target = "x86_64-pc-windows-msvc"
$Asset = "soulsystem-$Target.zip"
$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repository/releases/latest/download"
} else {
    "https://github.com/$Repository/releases/download/$Version"
}

$TemporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("soulsystem-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $TemporaryDirectory | Out-Null

try {
    $Archive = Join-Path $TemporaryDirectory $Asset
    $Checksum = "$Archive.sha256"
    Write-Host "Downloading $BaseUrl/$Asset"
    Invoke-WebRequest "$BaseUrl/$Asset" -OutFile $Archive
    Invoke-WebRequest "$BaseUrl/$Asset.sha256" -OutFile $Checksum

    $Expected = ((Get-Content $Checksum -Raw).Trim() -split "\s+")[0].ToLower()
    $Actual = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLower()
    if ($Expected -notmatch "^[a-f0-9]{64}$" -or $Expected -ne $Actual) {
        throw "Release checksum verification failed."
    }

    Expand-Archive $Archive -DestinationPath $TemporaryDirectory -Force
    $DownloadedBinary = Join-Path $TemporaryDirectory $Binary
    if (-not (Test-Path $DownloadedBinary -PathType Leaf)) {
        throw "The release archive does not contain $Binary."
    }

    New-Item -ItemType Directory -Path $InstallDirectory -Force | Out-Null
    Copy-Item $DownloadedBinary (Join-Path $InstallDirectory $Binary) -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathEntries = @($UserPath -split ";" | Where-Object { $_ })
    if ($InstallDirectory -notin $PathEntries) {
        $NewPath = (@($PathEntries) + $InstallDirectory) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        $env:Path = "$env:Path;$InstallDirectory"
        Write-Host "Added $InstallDirectory to your user PATH."
    }
    Write-Host "SoulSystem installed to $InstallDirectory\$Binary"
    Write-Host "Run 'soulsystem --help' to get started."
} finally {
    if (Test-Path $TemporaryDirectory) {
        Remove-Item $TemporaryDirectory -Recurse -Force
    }
}
