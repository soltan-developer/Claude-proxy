$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $projectRoot
try {
    pnpm tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri release build failed with exit code $LASTEXITCODE."
    }

    $sourceExe = Join-Path $projectRoot "src-tauri\target\release\claude-proxy.exe"
    if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
        throw "The expected release executable was not produced: $sourceExe"
    }

    $outputDirectory = Join-Path $projectRoot "portable"
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
    $outputExe = Join-Path $outputDirectory "Claude-Proxy-Portable.exe"
    Copy-Item -LiteralPath $sourceExe -Destination $outputExe -Force

    $file = Get-Item -LiteralPath $outputExe
    Write-Output "Portable executable: $($file.FullName)"
    Write-Output "Size: $([math]::Round($file.Length / 1MB, 2)) MB"
}
finally {
    Pop-Location
}
