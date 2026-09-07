# build.ps1
# Deterministic build script for SpoofDPI v1.2.1 engine
# Verified Commit: de9aab76cbb601d3c21077c776e21162d72d2b98

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$SpoofDpiDir = Join-Path $Root "SpoofDPI"
$BinDir = Join-Path $Root "bin"
$OutExe = Join-Path $BinDir "ozii-dpi-engine.exe"
$ExpectedCommit = "de9aab76cbb601d3c21077c776e21162d72d2b98"

Write-Host "Building SpoofDPI v1.2.1 from source..."

if (!(Test-Path (Join-Path $SpoofDpiDir ".git"))) {
    if (Test-Path $SpoofDpiDir) {
        $existingFiles = @(Get-ChildItem -LiteralPath $SpoofDpiDir -Force)
        if ($existingFiles.Count -gt 0) {
            throw "SpoofDPI directory exists but is not a git checkout: $SpoofDpiDir"
        }
        Remove-Item -LiteralPath $SpoofDpiDir -Force
    }

    Write-Host "Cloning the pinned SpoofDPI source..."
    & git clone https://github.com/xvzc/SpoofDPI.git $SpoofDpiDir
    if ($LASTEXITCODE -ne 0) {
        throw "SpoofDPI clone failed."
    }
}

if (!(Test-Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir | Out-Null
}

Push-Location $SpoofDpiDir
try {
    $CurrentCommit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Could not read the current SpoofDPI commit."
    }

    if ($CurrentCommit -ne $ExpectedCommit) {
        Write-Host "Checking out pinned SpoofDPI commit: $ExpectedCommit"
        & git fetch --depth 1 origin $ExpectedCommit
        if ($LASTEXITCODE -ne 0) { throw "SpoofDPI fetch failed." }
        & git checkout --detach $ExpectedCommit
        if ($LASTEXITCODE -ne 0) { throw "SpoofDPI checkout failed." }
    }

    $CurrentCommit = (& git rev-parse HEAD).Trim()
    if ($CurrentCommit -ne $ExpectedCommit) {
        throw "Build failed: expected SpoofDPI commit $ExpectedCommit but found $CurrentCommit."
    }
    Write-Host "Verified SpoofDPI commit: $ExpectedCommit"

    # Build with stripped debug symbols and reproducible source paths.
    Write-Host "Running go build..."
    & go build -trimpath -ldflags="-s -w" -o $OutExe ./cmd/spoofdpi
    if ($LASTEXITCODE -ne 0) { throw "SpoofDPI build failed." }
}
finally {
    Pop-Location
}

if (Test-Path $OutExe) {
    Write-Host "Success: $OutExe"
} else {
    Write-Error "Build failed. Binary not found."
}
