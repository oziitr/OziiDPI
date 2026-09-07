$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    Push-Location backend
    try {
        cargo fmt --all --check
        if ($LASTEXITCODE) { throw 'Backend formatting failed' }
        cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
        if ($LASTEXITCODE) { throw 'Backend clippy failed' }
        cargo test --workspace --all-features --locked
        if ($LASTEXITCODE) { throw 'Backend tests failed' }
    } finally { Pop-Location }
    & npm.cmd --prefix app run build
    if ($LASTEXITCODE) { throw 'Frontend build failed' }
    cargo test --manifest-path app/src-tauri/Cargo.toml --lib --locked
    if ($LASTEXITCODE) { throw 'Desktop and Discord discovery tests failed' }
    if (Test-Path 'engine/SpoofDPI/go.mod') {
        Push-Location 'engine/SpoofDPI'
        try {
            go vet ./...
            if ($LASTEXITCODE) { throw 'Engine vet failed' }
            go test ./...
            if ($LASTEXITCODE) { throw 'Engine tests failed' }
        } finally { Pop-Location }
    } else { Write-Host 'Engine source not present: run engine/build.ps1 to also test upstream Go code.' }
    Write-Host 'All available tests passed.'
} finally { Pop-Location }
