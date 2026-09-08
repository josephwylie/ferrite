[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $OutputFile,

    [Parameter(Mandatory = $true)]
    [string] $CertificateThumbprint,

    [string] $TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$manifest = Join-Path $root "Cargo.toml"
$installerScript = Join-Path $root "installer\windows\ferrite.iss"

cargo build --release --locked --manifest-path $manifest -p ferrite
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$metadata = cargo metadata --no-deps --format-version 1 --manifest-path $manifest | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
$target = $metadata.target_directory
$version = ($metadata.packages | Where-Object name -eq "ferrite").version
$binary = Join-Path $target "release\ferrite.exe"
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "No release binary at $binary"
}

$signTool = Get-Command signtool.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1
if (-not $signTool) {
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $signTool = Get-ChildItem -LiteralPath $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object FullName -Match '\\x64\\signtool\.exe$' |
        Sort-Object FullName -Descending |
        Select-Object -ExpandProperty FullName -First 1
}
if (-not $signTool) { throw "signtool.exe was not found" }

$isccCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 7\ISCC.exe")
)
$iscc = $isccCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $iscc) { throw "ISCC.exe was not found; install Inno Setup 6 or 7" }

& $signTool sign /sha1 $CertificateThumbprint /fd SHA256 /td SHA256 /tr $TimestampUrl $binary
if ($LASTEXITCODE -ne 0) { throw "Signing ferrite.exe failed" }

$outputPath = [IO.Path]::GetFullPath($OutputFile, (Get-Location).Path)
$outputDir = [IO.Path]::GetDirectoryName($outputPath)
$outputBase = [IO.Path]::GetFileNameWithoutExtension($outputPath)
[IO.Directory]::CreateDirectory($outputDir) | Out-Null

# Inno signs both Setup and its embedded uninstaller. Its $f token must remain
# literal here so ISCC can replace it with the file currently being signed.
$signCommand = "`"$signTool`" sign /sha1 $CertificateThumbprint /fd SHA256 /td SHA256 /tr $TimestampUrl `$f"
& $iscc "/Sauthenticode=$signCommand" "/DMyAppVersion=$version" "/DMyAppBinary=$binary" "/DMyAppRoot=$root" "/DMyAppOutputDir=$outputDir" "/DMyAppOutputBaseFilename=$outputBase" $installerScript
if ($LASTEXITCODE -ne 0) { throw "Compiling the Windows installer failed" }

if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
    throw "Installer was not created at $outputPath"
}
& $signTool verify /pa /all /v $binary
if ($LASTEXITCODE -ne 0) { throw "ferrite.exe signature verification failed" }
& $signTool verify /pa /all /v $outputPath
if ($LASTEXITCODE -ne 0) { throw "Installer signature verification failed" }

Write-Host "Packaged -> $outputPath"
