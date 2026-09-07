$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "========================================="
Write-Host "OziiDPI Safe Clean Script"
Write-Host "========================================="
Write-Host "Cleaning Rust target directories..."

if (Test-Path "$ProjectRoot\backend\target") {
    Remove-Item -Recurse -Force "$ProjectRoot\backend\target"
}
if (Test-Path "$ProjectRoot\app\src-tauri\target") {
    Remove-Item -Recurse -Force "$ProjectRoot\app\src-tauri\target"
}

Write-Host "Cleaning Node modules..."
if (Test-Path "$ProjectRoot\app\node_modules") {
    Remove-Item -Recurse -Force "$ProjectRoot\app\node_modules"
}
if (Test-Path "$ProjectRoot\app\dist") {
    Remove-Item -Recurse -Force "$ProjectRoot\app\dist"
}

Write-Host "Cleaning dist..."
if (Test-Path "$ProjectRoot\dist") {
    Remove-Item -Recurse -Force "$ProjectRoot\dist"
}

Write-Host "`nClean complete. Runtime state in LOCALAPPDATA was untouched."
