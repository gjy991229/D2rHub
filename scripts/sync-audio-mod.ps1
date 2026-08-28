param(
    [string]$GeneratorRepo = (Join-Path $PSScriptRoot "..\..\d2r-audio-mod")
)

$generatorRoot = (Resolve-Path -LiteralPath $GeneratorRepo -ErrorAction Stop).Path
$manifestPath = Join-Path $generatorRoot "Cargo.toml"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Standalone generator repository not found: $generatorRoot"
}

& cargo build --release --manifest-path $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "Standalone generator build failed"
}

$source = Join-Path $generatorRoot "target\release\d2r-audio-mod.exe"
$destinationDirectory = Join-Path $PSScriptRoot "..\src-tauri\binaries"
$destination = Join-Path $destinationDirectory "d2r-audio-mod-x86_64-pc-windows-msvc.exe"
New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
Copy-Item -LiteralPath $source -Destination $destination -Force
Write-Host "Updated D2RHub sidecar: $destination"
