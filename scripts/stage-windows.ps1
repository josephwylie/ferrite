# The runnable Windows layout, shared by the installer and CI. With visuals,
# CEF's sandbox bootstrap is ferrite.exe and the Rust application is ferrite.dll.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $BuildDirectory,

    [Parameter(Mandatory = $true)]
    [string] $Destination,

    [switch] $NoVisuals
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$application = Join-Path $BuildDirectory $(if ($NoVisuals) { "ferrite.exe" } else { "ferrite.dll" })
if (-not (Test-Path -LiteralPath $application -PathType Leaf)) {
    throw "No built application at $application"
}
if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
New-Item -ItemType Directory -Path $Destination | Out-Null
Copy-Item -LiteralPath $application -Destination $Destination
if (-not $NoVisuals) {
    $cef = & (Join-Path $PSScriptRoot "copy-chromium-windows.ps1") -Destination $Destination -PassThru
    $bootstrap = Join-Path $cef "bootstrap.exe"
    if (-not (Test-Path -LiteralPath $bootstrap -PathType Leaf)) {
        throw "CEF sandbox bootstrap not found: $bootstrap"
    }
    Copy-Item -LiteralPath $bootstrap -Destination (Join-Path $Destination "ferrite.exe")
    # CEF's prebuilt bootstrap owns the main thread, including Rust's Window
    # layout. Both it and the ordinary Rust EXE overflowed the default 1 MiB
    # stack in run 42567297438 (0xC00000FD). Match build.rs's 8 MiB reserve,
    # retaining the 4 KiB initial commit. EDITBIN is Microsoft's supported
    # post-link /STACK setting; modify only our copy, before package signing.
    # https://learn.microsoft.com/en-us/cpp/build/reference/stack
    $editbin = Get-Command editbin.exe -ErrorAction SilentlyContinue
    if ($editbin) {
        $editbinPath = $editbin.Source
    } else {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
        if (-not (Test-Path -LiteralPath $vswhere)) { throw "Visual Studio vswhere.exe is required to locate EDITBIN" }
        $editbinPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\editbin.exe' |
            Select-Object -First 1
        if (-not $editbinPath) { throw "Visual Studio x64 EDITBIN was not found" }
    }
    & $editbinPath /NOLOGO /STACK:8388608,4096 (Join-Path $Destination "ferrite.exe")
    if ($LASTEXITCODE -ne 0) { throw "Setting the packaged Windows main-thread stack failed" }
}
Write-Host "Staged runnable Ferrite -> $Destination"
