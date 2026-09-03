param(
    [string]$ExePath = "src-tauri/target/release/d2rhub.exe",
    [int]$WarmupSeconds = 8,
    [string]$BudgetPath = "scripts/release-performance-budget.json",
    [string]$OutputPath = "",
    [switch]$ElevatedChild
)

$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
$isAdministrator = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdministrator -and -not $ElevatedChild) {
    $absoluteExe = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $ExePath))
    $absoluteBudget = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $BudgetPath))
    $temporaryOutput = -not $OutputPath
    $absoluteOutput = if ($temporaryOutput) {
        Join-Path ([System.IO.Path]::GetTempPath()) "d2rhub-release-baseline-$PID.json"
    } else {
        [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputPath))
    }
    $arguments = @(
        "-NoProfile",
        "-ExecutionPolicy Bypass",
        "-File `"$PSCommandPath`"",
        "-ExePath `"$absoluteExe`"",
        "-WarmupSeconds $WarmupSeconds",
        "-BudgetPath `"$absoluteBudget`"",
        "-OutputPath `"$absoluteOutput`"",
        "-ElevatedChild"
    ) -join " "
    $helper = Start-Process -FilePath powershell.exe -Verb RunAs -ArgumentList $arguments `
        -WindowStyle Hidden -PassThru -Wait
    if (Test-Path -LiteralPath $absoluteOutput) {
        Get-Content -LiteralPath $absoluteOutput -Raw
        if ($temporaryOutput) { Remove-Item -LiteralPath $absoluteOutput }
    }
    if ($helper.ExitCode -ne 0) {
        throw "Elevated release baseline failed with exit code $($helper.ExitCode)."
    }
    exit 0
}
if ($ElevatedChild -and -not $isAdministrator) {
    throw "Release baseline elevation was not granted."
}

$resolvedExe = (Resolve-Path -LiteralPath $ExePath).Path
$existing = Get-CimInstance Win32_Process -Filter "Name = 'd2rhub.exe'" |
    Where-Object { $_.ExecutablePath -eq $resolvedExe }
if ($existing) {
    throw "The release executable is already running; close it before measuring."
}

function Get-ProtectedFileSnapshot([string]$RoamingRoot) {
    $appData = Join-Path $RoamingRoot "D2RHub"
    if (-not (Test-Path -LiteralPath $appData)) { return @{} }
    $snapshot = @{}
    Get-ChildItem -LiteralPath $appData -File -Recurse |
        Where-Object {
            $_.FullName -notmatch "[\\/](logs|diagnostics)[\\/]"
        } |
        ForEach-Object {
            $sha256 = [System.Security.Cryptography.SHA256]::Create()
            $stream = [System.IO.File]::OpenRead($_.FullName)
            try {
                $bytes = $sha256.ComputeHash($stream)
                $snapshot[$_.FullName] = [System.BitConverter]::ToString($bytes).Replace("-", "")
            } finally {
                $stream.Dispose()
                $sha256.Dispose()
            }
        }
    return $snapshot
}

$realRoamingRoot = [Environment]::GetFolderPath("ApplicationData")
$baselineRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("d2rhub-release-baseline-{0}" -f [Guid]::NewGuid().ToString("N"))
$isolatedRoamingRoot = Join-Path $baselineRoot "Roaming"
New-Item -ItemType Directory -Path $isolatedRoamingRoot -Force | Out-Null

$beforeFiles = Get-ProtectedFileSnapshot $realRoamingRoot
$beforeD2r = @(Get-Process -Name "D2R" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$previousAppData = $env:APPDATA
try {
    $env:APPDATA = $isolatedRoamingRoot
    $process = Start-Process -FilePath $resolvedExe -PassThru -WindowStyle Hidden
} finally {
    $env:APPDATA = $previousAppData
}
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw "D2RHub exited before its main window became ready." }
    } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)
    $startupMs = $stopwatch.ElapsedMilliseconds
    Start-Sleep -Seconds $WarmupSeconds
    $process.Refresh()
    $cpuStart = $process.TotalProcessorTime
    $cpuClock = [System.Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 3
    $process.Refresh()
    $cpuPercent = (($process.TotalProcessorTime - $cpuStart).TotalMilliseconds /
        $cpuClock.Elapsed.TotalMilliseconds / [Environment]::ProcessorCount) * 100

    $directChildren = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)")
    $afterD2r = @(Get-Process -Name "D2R" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
    $result = [ordered]@{
        measured_at_utc = [DateTime]::UtcNow.ToString("o")
        executable = $resolvedExe
        startup_to_window_ms = $startupMs
        warm_idle_cpu_percent = [Math]::Round($cpuPercent, 2)
        working_set_mib = [Math]::Round($process.WorkingSet64 / 1MB, 1)
        private_memory_mib = [Math]::Round($process.PrivateMemorySize64 / 1MB, 1)
        thread_count = $process.Threads.Count
        direct_child_process_count = $directChildren.Count
        main_window_ready = $process.MainWindowHandle -ne 0
        d2r_process_set_unchanged = (@(Compare-Object $beforeD2r $afterD2r).Count -eq 0)
        app_data_isolated = $true
    }
} finally {
    if (-not $process.HasExited) {
        try {
            Stop-Process -Id $process.Id -Force
        } catch {
            # Elevated Tauri processes can report access denied while the
            # termination request is still being completed. Verify the exact
            # PID before treating that response as a cleanup failure.
            Start-Sleep -Milliseconds 750
            if (Get-Process -Id $process.Id -ErrorAction SilentlyContinue) {
                throw
            }
        }
        $process.WaitForExit(5000) | Out-Null
    }
}

$afterFiles = Get-ProtectedFileSnapshot $realRoamingRoot
$fileChanges = @(Compare-Object ($beforeFiles.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) `
    ($afterFiles.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }))
$result.protected_files_unchanged = $fileChanges.Count -eq 0
$budget = Get-Content -LiteralPath $BudgetPath -Raw | ConvertFrom-Json
$budgetPassed =
    $result.startup_to_window_ms -le $budget.max_startup_to_window_ms -and
    $result.warm_idle_cpu_percent -le $budget.max_warm_idle_cpu_percent -and
    $result.working_set_mib -le $budget.max_working_set_mib -and
    $result.private_memory_mib -le $budget.max_private_memory_mib -and
    $result.thread_count -le $budget.max_thread_count -and
    $result.direct_child_process_count -le $budget.max_direct_child_process_count
$result.performance_budget_passed = $budgetPassed
$json = $result | ConvertTo-Json
if ($OutputPath) {
    $parent = Split-Path -Parent $OutputPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent | Out-Null
    }
    Set-Content -LiteralPath $OutputPath -Value $json -Encoding UTF8
}
$json

$resolvedBaselineRoot = [System.IO.Path]::GetFullPath($baselineRoot)
$resolvedTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
if ($resolvedBaselineRoot.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $resolvedBaselineRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $result.main_window_ready -or
    -not $result.d2r_process_set_unchanged -or
    -not $result.protected_files_unchanged -or
    -not $result.performance_budget_passed) {
    throw "Release smoke baseline failed its no-side-effect guard."
}
