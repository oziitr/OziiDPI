$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "========================================="
Write-Host "OziiDPI Dev Startup Script"
Write-Host "========================================="

Set-Location "$ProjectRoot\app"
npm run tauri dev
