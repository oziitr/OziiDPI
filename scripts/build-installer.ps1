$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir

& (Join-Path $scriptDir "build-discord-only.ps1")
if ($LASTEXITCODE -ne 0) { throw "OziiDPI product build failed" }

$compilerCandidates = @(
    (Join-Path $projectRoot "dist\tools\InnoSetup\ISCC.exe"),
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 7\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 7\ISCC.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe")
)
$compiler = $compilerCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $compiler) {
    throw "Inno Setup compiler not found"
}

& $compiler (Join-Path $projectRoot "installer\OziiDPI.iss")
if ($LASTEXITCODE -ne 0) { throw "OziiDPI installer build failed" }

Write-Host "OziiDPI installer ready: $(Join-Path $projectRoot 'dist\installer\OziiDPI-Setup.exe')"
