$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "========================================="
Write-Host "OziiDPI Full App Release Build Script"
Write-Host "========================================="

# 1. Build Go Engine
Write-Host "`n[1/5] Building SpoofDPI Engine (Go)..."
Set-Location "$ProjectRoot\engine\SpoofDPI"
go build -ldflags "-s -w -buildid=" -trimpath -o "$ProjectRoot\engine\bin\ozii-dpi-engine.exe" ./cmd/spoofdpi
if ($LASTEXITCODE -ne 0) { throw "Engine build failed" }

# 2. Build Rust Backend
Write-Host "`n[2/5] Building Rust Backend CLI..."
Set-Location "$ProjectRoot\backend"
cargo build --release --workspace
if ($LASTEXITCODE -ne 0) { throw "Backend build failed" }

# 3. Assemble Dist
Write-Host "`n[3/5] Assembling Dist Directory..."
$DistDir = "$ProjectRoot\dist\backend"
if (Test-Path $DistDir) {
    Remove-Item -Recurse -Force $DistDir
}
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
Copy-Item "$ProjectRoot\engine\bin\ozii-dpi-engine.exe" -Destination "$DistDir\ozii-dpi-engine.exe"
Copy-Item "$ProjectRoot\backend\target\release\ozii-cli.exe" -Destination "$DistDir\ozii-cli.exe"
Copy-Item "$ProjectRoot\backend\target\release\ozii-guard.exe" -Destination "$DistDir\ozii-guard.exe"
Copy-Item "$ProjectRoot\backend\target\release\WinDivert.dll" -Destination "$DistDir\WinDivert.dll"
Copy-Item "$ProjectRoot\backend\target\release\WinDivert64.sys" -Destination "$DistDir\WinDivert64.sys"

# 4. Build Frontend Tauri
Write-Host "`n[4/5] Building Tauri Frontend App..."
Set-Location "$ProjectRoot\app"
npm install
npm run build
npm run tauri build
if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }

# 5. Hashes
Write-Host "`n[5/5] Generating Hashes..."
Set-Location $DistDir
$EngineHash = (Get-FileHash "ozii-dpi-engine.exe" -Algorithm SHA256).Hash
$CliHash = (Get-FileHash "ozii-cli.exe" -Algorithm SHA256).Hash

$HashContent = @"
SHA256(ozii-dpi-engine.exe) = $EngineHash
SHA256(ozii-cli.exe) = $CliHash
"@
Set-Content "hashes.txt" $HashContent

Write-Host "`nBuild complete!"
Write-Host "Tauri App is located in: $ProjectRoot\app\src-tauri\target\release\bundle\"
