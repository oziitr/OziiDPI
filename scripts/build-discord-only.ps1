$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$packageDir = Join-Path $projectRoot "dist\product"

Set-Location (Join-Path $projectRoot "backend")
cargo build --release --workspace --locked
if ($LASTEXITCODE -ne 0) { throw "Backend build failed" }

Set-Location (Join-Path $projectRoot "app")
& npm.cmd run tauri -- build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "OziiDPI desktop app build failed" }

if (Test-Path -LiteralPath $packageDir) {
    $allowedRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'dist')).TrimEnd('\') + '\'
    if (-not [IO.Path]::GetFullPath($packageDir).StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Unsafe product staging path'
    }
    Remove-Item -LiteralPath $packageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $packageDir -Force | Out-Null

$desktopExe = Join-Path $projectRoot "app\src-tauri\target\release\ozii-dpi.exe"
Copy-Item -LiteralPath $desktopExe -Destination (Join-Path $packageDir "OziiDPI.exe")
Copy-Item -LiteralPath (Join-Path $projectRoot "backend\target\release\ozii-cli.exe") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "backend\target\release\WinDivert.dll") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "engine\bin\ozii-dpi-engine.exe") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "DISCORD_ONLY_README.txt") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "THIRD_PARTY_NOTICES.md") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "backend\deps\windivert\LICENSE") -Destination (Join-Path $packageDir "WINDIVERT_LICENSE.txt")
Copy-Item -LiteralPath (Join-Path $projectRoot "installer\chrome-native-host.json") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $projectRoot "chrome-extension") -Destination $packageDir -Recurse

# Discord modules are resolved from the user's installed matching version at runtime.
# No proprietary Discord modules or hard-coded host version are distributed.

Get-ChildItem -LiteralPath $packageDir -Recurse -File | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    $relative = $_.FullName.Substring($packageDir.Length + 1)
    "{0}  {1}" -f $hash, $relative
} | Set-Content -LiteralPath (Join-Path $packageDir "SHA256SUMS.txt") -Encoding ascii

Write-Host "OziiDPI product package ready: $packageDir"
