# Packaging seam: stage the bootstrap and app DLL together, using the exact
# CEF runtime selected by Cargo.lock. Fake bytes test layout, not execution.
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$stageScript = Join-Path $PSScriptRoot "..\stage-windows.ps1"
$scratch = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$previousCefPath = $env:CEF_PATH
try {
    $build = Join-Path $scratch "build"
    $runtime = Join-Path $scratch "cef"
    $stage = Join-Path $scratch "stage"
    New-Item -ItemType Directory -Path $build, $runtime | Out-Null
    Set-Content -LiteralPath (Join-Path $build "ferrite.exe") -Value "ordinary executable"
    Set-Content -LiteralPath (Join-Path $build "ferrite.dll") -Value "application DLL"
    foreach ($file in @("bootstrap.exe", "libcef.dll", "chrome_elf.dll", "icudtl.dat", "v8_context_snapshot.bin", "resources.pak", "chrome_100_percent.pak", "chrome_200_percent.pak", "d3dcompiler_47.dll", "dxcompiler.dll", "dxil.dll", "vk_swiftshader.dll", "vk_swiftshader_icd.json", "vulkan-1.dll")) {
        Set-Content -LiteralPath (Join-Path $runtime $file) -Value $file
    }
    New-Item -ItemType Directory -Path (Join-Path $runtime "locales") | Out-Null
    Set-Content -LiteralPath (Join-Path $runtime "locales\en-US.pak") -Value "locale"
    $env:CEF_PATH = $runtime
    & $stageScript -BuildDirectory $build -Destination $stage
    if ((Get-Content -Raw (Join-Path $stage "ferrite.exe")).Trim() -ne "bootstrap.exe") { throw "Packaged executable must be CEF's bootstrap" }
    if ((Get-Content -Raw (Join-Path $stage "ferrite.dll")).Trim() -ne "application DLL") { throw "Application DLL was not staged" }
    if (-not (Test-Path -LiteralPath (Join-Path $stage "locales\en-US.pak"))) { throw "CEF locales were not staged" }
    Write-Host "PASS: sandbox bootstrap, application DLL and CEF runtime staged together"
    # A no-CEF package remains a standalone executable, including when
    # replacing a previously staged visuals build.
    & $stageScript -BuildDirectory $build -Destination $stage -NoVisuals
    if ((Get-Content -Raw (Join-Path $stage "ferrite.exe")).Trim() -ne "ordinary executable") { throw "No-CEF package must retain the ordinary executable" }
    if (Test-Path -LiteralPath (Join-Path $stage "ferrite.dll")) { throw "No-CEF package retained a stale application DLL" }
    if (Test-Path -LiteralPath (Join-Path $stage "libcef.dll")) { throw "No-CEF package retained a stale Chromium runtime" }
    Write-Host "PASS: no-CEF executable stages without stale Chromium files"
} finally {
    $env:CEF_PATH = $previousCefPath
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
