param(
    [string]$Replay,
    [string]$Scene,
    [string]$Output
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (!$Output) { $Output = Join-Path $repo ('target/gauntlet-' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
$arguments = @('run', '--offline', '--release', '--manifest-path', 'tools/audit-probe/Cargo.toml', '--target-dir', 'target', '--', '--live-gauntlet', '--out', $Output)
if ($Replay) { $arguments += @('--replay', (Resolve-Path -LiteralPath $Replay).Path) }
if ($Scene) { $arguments += @('--scene', $Scene) }
Push-Location -LiteralPath $repo
try {
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Visualizer gauntlet failed (exit $LASTEXITCODE)" }
    Write-Output "Review: $Output/index.html"
} finally { Pop-Location }
