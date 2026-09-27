param(
    [ValidateSet('software', 'processor', 'mods', 'all')][string]$Target,
    [switch]$Publish,
    [switch]$Promote,
    [string]$Resume,
    [string]$Config
)
$ErrorActionPreference = 'Stop'
$env:PYTHONUTF8 = '1'
$workflowArgs = @((Join-Path $PSScriptRoot 'scripts/release-workflow.py'))
if ($Target) { $workflowArgs += @('--target', $Target) }
if ($Publish) { $workflowArgs += '--publish' }
if ($Promote) { $workflowArgs += '--promote' }
if ($Resume) { $workflowArgs += @('--resume', $Resume) }
if ($Config) { $workflowArgs += @('--config', $Config) }
& python @workflowArgs
exit $LASTEXITCODE
