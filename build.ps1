$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read package version.' }
    $version = ($metadata.packages | Where-Object name -eq 'world-clock').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a stable package version.' }
    $executableName = "WorldClock-v$version.exe"
    $destination = Join-Path $PSScriptRoot 'dist'
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    $executablePath = Join-Path $destination $executableName
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\release\world-clock.exe') -Destination $executablePath -Force
    $exeHash = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$exeHash  $executableName" | Set-Content -LiteralPath (Join-Path $destination 'SHA256SUMS.txt') -Encoding ascii
    Get-Item -LiteralPath $executablePath | Select-Object FullName, Length
} finally {
    Pop-Location
}
