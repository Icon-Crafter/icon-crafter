# Builds the production frontend and the Windows installers (NSIS and MSI).
#
# Requirements: Rust stable (MSVC toolchain), Node 22+, pnpm, Windows 10/11 with
# the WebView2 runtime. The Tauri CLI comes from apps/ui devDependencies, so run
# `pnpm --dir apps/ui install` once before using this script.
#
# Usage:
#   pwsh tools/package-windows.ps1              # NSIS + MSI
#   pwsh tools/package-windows.ps1 -Bundles nsis
#   pwsh tools/package-windows.ps1 -Bundles msi
param(
    [ValidateSet('nsis', 'msi', 'both')]
    [string]$Bundles = 'both'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    Write-Host '[1/3] Building the production frontend bundle (apps/ui/dist)...'
    pnpm --dir apps/ui build
    if ($LASTEXITCODE -ne 0) { throw 'pnpm build failed' }

    $tauri = Join-Path $repo 'apps\ui\node_modules\.bin\tauri.cmd'
    if (-not (Test-Path $tauri)) {
        throw 'Tauri CLI missing: run `pnpm --dir apps/ui install` (or `pnpm --dir apps/ui add -D @tauri-apps/cli@^2`).'
    }

    $bundleArg = @()
    if ($Bundles -ne 'both') { $bundleArg = @('--bundles', $Bundles) }

    Write-Host '[2/3] Running tauri build (the CLI enables the custom-protocol feature for production)...'
    Push-Location (Join-Path $repo 'apps\desktop')
    try {
        $env:CARGO_INCREMENTAL = '0'
        & $tauri build @bundleArg
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }
    }
    finally { Pop-Location }

    Write-Host '[3/3] Installer artifacts:'
    Get-ChildItem (Join-Path $repo 'target\release\bundle') -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in '.exe', '.msi' } |
        Select-Object FullName, Length
}
finally { Pop-Location }
