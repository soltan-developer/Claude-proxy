# Run-CargoTest.ps1 — one-click cargo test for Claude Proxy
# Usage:  powershell -NoProfile -ExecutionPolicy Bypass -File .\Run-CargoTest.ps1
# Output: cargo-test-output.txt next to this script (read it from Claude).

# NOTE: do NOT use "Stop" here — cargo writes progress to stderr, and a
# stopping preference would abort the script on the first stderr line.
$ErrorActionPreference = "Continue"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location (Join-Path $root "src-tauri")

$output = Join-Path $root "cargo-test-output.txt"
$log    = Join-Path $root "cargo-test-progress.log"

Write-Host "Running cargo test (this can take a few minutes on first run)..." -ForegroundColor Cyan
cargo test 2>&1 | Out-File -FilePath $log -Encoding utf8
$code = $LASTEXITCODE

Copy-Item -Path $log -Destination $output -Force

Add-Content -Path $output -Value "" -Encoding utf8
Add-Content -Path $output -Value "=== EXIT CODE: $code ===" -Encoding utf8

if ($code -eq 0) {
    Write-Host "PASS (exit 0). Output: $output" -ForegroundColor Green
} else {
    Write-Host "FAIL (exit $code). Output: $output" -ForegroundColor Red
}
