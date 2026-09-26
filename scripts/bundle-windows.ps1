# Builds dist\Kerf-<version>-windows-x86_64.zip (kerf.exe with embedded icon + README).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$name = "Kerf-$version-windows-x86_64"
$stage = "dist\$name"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item target\release\kerf.exe $stage
Copy-Item README.md $stage
Compress-Archive -Path "$stage\*" -DestinationPath "dist\$name.zip" -Force
Write-Output "built dist\$name.zip"
