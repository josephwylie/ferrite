# Put Chromium (CEF) beside a Windows ferrite.exe built with `--features cef`
# (ADR 0013): libcef.dll, the files it loads at run time, and locales\.
#
# Taken from the CEF distribution cef-dll-sys fetched for the `cef` crate in
# Cargo.lock, under $env:CEF_PATH (default ~\.local\share\cef), never from
# the target directory: cef-dll-sys copies the runtime there only when its
# build script runs, which a restored build cache skips.
#
#   copy-chromium-windows.ps1 -Destination <directory holding ferrite.exe>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $Destination
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# What CEF's README ("REDISTRIBUTION") and its cmake (CEF_BINARY_FILES,
# CEF_RESOURCE_FILES) put beside the executable, for CEF 154 on x64. The
# import library, bootstrap*.exe and the build files beside them stay out.
$runtime = @(
    "libcef.dll",
    "chrome_elf.dll",
    "icudtl.dat",
    "v8_context_snapshot.bin",
    "resources.pak",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "d3dcompiler_47.dll",
    "dxcompiler.dll",
    "dxil.dll",
    "vk_swiftshader.dll",
    "vk_swiftshader_icd.json",
    "vulkan-1.dll",
    "locales"
)

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cefPath = if ($env:CEF_PATH) { $env:CEF_PATH } else { Join-Path $HOME ".local\share\cef" }

# crate 154.5.0+154.0.34 -> CEF 154.0.34
$lock = Get-Content -Raw -LiteralPath (Join-Path $root "Cargo.lock")
if ($lock -notmatch '(?m)^name = "cef"\r?\nversion = "[^"+]*\+([^"]+)"') {
    throw "No cef crate in Cargo.lock"
}
$version = $Matches[1]

$cef = @((Join-Path $cefPath "$version\cef_windows_x86_64"), $cefPath) |
    Where-Object { Test-Path -LiteralPath (Join-Path $_ "libcef.dll") } |
    Select-Object -First 1
if (-not $cef) { throw "CEF $version for Windows not found under $cefPath (build with --features cef first)" }

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
foreach ($name in $runtime) {
    $path = Join-Path $cef $name
    if (-not (Test-Path -LiteralPath $path)) { throw "CEF $version has no $name ($cef)" }
    $target = Join-Path $Destination $name
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    Copy-Item -LiteralPath $path -Destination $Destination -Recurse
}
Write-Host "Chromium (CEF $version) -> $Destination"
