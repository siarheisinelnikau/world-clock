$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
    $destination = Join-Path $PSScriptRoot 'dist'
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\release\world-clock.exe') -Destination (Join-Path $destination 'WorldClock.exe') -Force
    $exeHash = (Get-FileHash -LiteralPath (Join-Path $destination 'WorldClock.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    "$exeHash  WorldClock.exe" | Set-Content -LiteralPath (Join-Path $destination 'SHA256SUMS.txt') -Encoding ascii
    Get-Item -LiteralPath (Join-Path $destination 'WorldClock.exe') | Select-Object FullName, Length
} finally {
    Pop-Location
}
