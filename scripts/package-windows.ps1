[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $OutputFile,

    [string] $CertificateThumbprint,

    [string] $TimestampUrl = "http://timestamp.digicert.com"
)

# Visuals (ADR 0013): Ferrite is built with `--features cef`, and the
# installer carries Chromium (CEF) beside ferrite.exe: libcef.dll, its
# resources and locales\ (`copy-chromium-windows.ps1`). The binary is its own
# Chromium helper (`visual::boot` spots `--type=`). Building CEF's C++
# wrapper needs cmake and ninja on PATH; CEF itself (~170 MB download, ~400 MB
# of it installed) is fetched once into $env:CEF_PATH (default
# ~\.local\share\cef). FERRITE_NO_CEF=1 builds without visuals.

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $root "Cargo.toml"
$installerScript = Join-Path $root "installer\windows\ferrite.iss"

$visuals = -not $env:FERRITE_NO_CEF
$features = @()
if ($visuals) {
    foreach ($tool in "cmake", "ninja") {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "$tool not found (needed to build Chromium Embedded Framework's wrapper); or set FERRITE_NO_CEF=1 to build without visuals"
        }
    }
    if (-not $env:CEF_PATH) { $env:CEF_PATH = Join-Path $HOME ".local\share\cef" }
    $features = @("--features", "cef")
}

cargo build --release --locked --manifest-path $manifest -p ferrite @features
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$metadata = cargo metadata --no-deps --format-version 1 --manifest-path $manifest | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
$target = $metadata.target_directory
$version = ($metadata.packages | Where-Object name -eq "ferrite").version
$built = Join-Path $target "release\ferrite.exe"
if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
    throw "No release binary at $built"
}

# The install directory, staged: the binary, and Chromium beside it.
$stage = Join-Path $target "release\bundle\windows\Ferrite"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null
$binary = Join-Path $stage "ferrite.exe"
Copy-Item -LiteralPath $built -Destination $binary
if ($visuals) {
    & (Join-Path $PSScriptRoot "copy-chromium-windows.ps1") -Destination $stage
}
$size = (Get-ChildItem -LiteralPath $stage -Recurse -File | Measure-Object -Sum Length).Sum
Write-Host ("Staged {0:N0} MB -> {1}" -f ($size / 1MB), $stage)

$isccCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 7\ISCC.exe")
)
$iscc = $isccCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $iscc) { throw "ISCC.exe was not found; install Inno Setup 6 or 7" }

$outputPath = [IO.Path]::GetFullPath($OutputFile, (Get-Location).Path)
$outputDir = [IO.Path]::GetDirectoryName($outputPath)
$outputBase = [IO.Path]::GetFileNameWithoutExtension($outputPath)
[IO.Directory]::CreateDirectory($outputDir) | Out-Null

$isccArguments = @(
    "/DMyAppVersion=$version",
    "/DMyAppStage=$stage",
    "/DMyAppRoot=$root",
    "/DMyAppOutputDir=$outputDir",
    "/DMyAppOutputBaseFilename=$outputBase"
)

$signed = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
if ($signed) {
    $signTool = Get-Command signtool.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1
    if (-not $signTool) {
        $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
        $signTool = Get-ChildItem -LiteralPath $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
            Where-Object FullName -Match '\\x64\\signtool\.exe$' |
            Sort-Object FullName -Descending |
            Select-Object -ExpandProperty FullName -First 1
    }
    if (-not $signTool) { throw "signtool.exe was not found" }

    & $signTool sign /sha1 $CertificateThumbprint /fd SHA256 /td SHA256 /tr $TimestampUrl $binary
    if ($LASTEXITCODE -ne 0) { throw "Signing ferrite.exe failed" }

    # Inno signs both Setup and its embedded uninstaller. Its $f token must
    # remain literal here so ISCC can replace it with the current file.
    $signCommand = "`"$signTool`" sign /sha1 $CertificateThumbprint /fd SHA256 /td SHA256 /tr $TimestampUrl `$f"
    $isccArguments += "/Sauthenticode=$signCommand"
    $isccArguments += "/DMySignedBuild=1"
}

& $iscc @isccArguments $installerScript
if ($LASTEXITCODE -ne 0) { throw "Compiling the Windows installer failed" }

if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
    throw "Installer was not created at $outputPath"
}
if ($signed) {
    & $signTool verify /pa /all /v $binary
    if ($LASTEXITCODE -ne 0) { throw "ferrite.exe signature verification failed" }
    & $signTool verify /pa /all /v $outputPath
    if ($LASTEXITCODE -ne 0) { throw "Installer signature verification failed" }
}

Write-Host "Packaged -> $outputPath"
