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
}
Write-Host "Staged runnable Ferrite -> $Destination"
