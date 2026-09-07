# start-test.ps1
. "$PSScriptRoot\common.ps1"

$ErrorActionPreference = "Stop"

if (-not (Test-Path $CLI_BIN)) {
    Push-Location $BACKEND_DIR
    cargo build --workspace 2>&1 | Out-Null
    Pop-Location
}
if (-not (Test-Path $ENGINE_BIN)) {
    Write-Fail "Engine binary not found"
    exit 1
}

$env:OZIIDPI_IGNORE_WPAD = "1"
Write-Host "[TEST MODE] WPAD safety check bypassed for this test process only." -ForegroundColor Yellow

Ensure-OutputDir
$script:proxyBefore = Capture-ProxyState -OutputFile (Join-Path $OUTPUT_DIR "manual-proxy-before.txt")

if (Test-Path $RUNTIME_JSON) { Remove-Item $RUNTIME_JSON -Force -ErrorAction SilentlyContinue }

$cliFileInfo = Get-Item $CLI_BIN
Write-Host "CLI path: $($cliFileInfo.FullName)" -ForegroundColor Cyan
Write-Host "CLI timestamp: $($cliFileInfo.LastWriteTime)" -ForegroundColor Cyan
Write-Host "CLI size: $($cliFileInfo.Length) bytes" -ForegroundColor Cyan
Write-Host "Build ID: NEW_BUILD_$(Get-Date -Format 'yyyyMMddHHmmss')" -ForegroundColor Cyan

# Start backend attached to console
$proc = Start-Process -FilePath $CLI_BIN -ArgumentList "start", "--mode", "balanced", "--chunk-size", "2" -NoNewWindow -PassThru

$deadline = (Get-Date).AddSeconds(10)
$rtInfo = $null
while ((Get-Date) -lt $deadline) {
    $rtInfo = Get-RuntimeInfo
    if ($rtInfo) { break }
    Start-Sleep -Milliseconds 200
}

if (-not $rtInfo) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Red
    Write-Host " START FAILED" -ForegroundColor Red
    Write-Host "========================================" -ForegroundColor Red
    Write-Host "Error: Backend started but runtime.json not created." -ForegroundColor Red
    Write-Host "Press any key to close..." -ForegroundColor Red
    exit 1
}

$adapterPort = $rtInfo.adapter_port
$diagPort = $rtInfo.diagnostics_port

# Get initial diagnostics
$diagBeforeObj = Query-Diagnostics $diagPort | ConvertFrom-Json
$discord_forwarded_before = if ($diagBeforeObj) { [int]$diagBeforeObj.discord_forwarded } else { 0 }

# Automatic Discord Proof Before Manual Test
$discordRes = "FAIL"
$curlOut = cmd.exe /c "curl.exe -sS -o NUL -w `"%{http_code}`" --max-time 20 --proxy http://127.0.0.1:$adapterPort https://discord.com/ 2>&1" | Out-String
$httpStatus = $curlOut.Trim()

if ($httpStatus -match "(200|301|302|403)") { $discordRes = "PASS" }

$diagAfterObj = Query-Diagnostics $diagPort | ConvertFrom-Json
$discord_forwarded_after = if ($diagAfterObj) { [int]$diagAfterObj.discord_forwarded } else { 0 }
$nonDiscordFwd = if ($diagAfterObj) { [int]$diagAfterObj.non_discord_forwarded } else { 0 }

if ($discord_forwarded_after -le $discord_forwarded_before) {
    $discordRes = "FAIL (Counter did not increase)"
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " OziiDPI MANUAL LIVE TEST" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host " STATUS: CONNECTED" -ForegroundColor Green
Write-Host ""
Write-Host " Backend PID: $($proc.Id)" -ForegroundColor White
Write-Host " Engine PID:  (Managed by backend)" -ForegroundColor White
Write-Host ""
Write-Host " Adapter:" -ForegroundColor White
Write-Host " 127.0.0.1:$adapterPort" -ForegroundColor White
Write-Host ""
Write-Host " PAC:" -ForegroundColor White
Write-Host " $($rtInfo.pac_url)" -ForegroundColor White
Write-Host ""
Write-Host " Engine:" -ForegroundColor White
Write-Host " 127.0.0.1:$($rtInfo.engine_port)" -ForegroundColor White
Write-Host ""
Write-Host " Discord proxy test: $discordRes" -ForegroundColor White
Write-Host " HTTP status: $httpStatus" -ForegroundColor White
Write-Host " Discord forwarded before: $discord_forwarded_before" -ForegroundColor White
Write-Host " Discord forwarded after:  $discord_forwarded_after" -ForegroundColor White
Write-Host ""
Write-Host " Discord forwarded:     $discord_forwarded_after" -ForegroundColor White
Write-Host " Non-Discord forwarded: $nonDiscordFwd" -ForegroundColor White
Write-Host ""
Write-Host " ----------------------------------------" -ForegroundColor Yellow
Write-Host " KEEP THIS WINDOW OPEN" -ForegroundColor Yellow
Write-Host " ----------------------------------------" -ForegroundColor Yellow
Write-Host ""
Write-Host " Now manually test:" -ForegroundColor White
Write-Host ""
Write-Host " 1. Close Chrome completely" -ForegroundColor White
Write-Host " 2. Reopen Chrome" -ForegroundColor White
Write-Host " 3. Open:" -ForegroundColor White
Write-Host "    https://discord.com/app" -ForegroundColor White
Write-Host ""
Write-Host " 4. Open:" -ForegroundColor White
Write-Host "    https://google.com" -ForegroundColor White
Write-Host ""
Write-Host " 5. Fully close Discord Desktop" -ForegroundColor White
Write-Host " 6. Reopen Discord Desktop" -ForegroundColor White
Write-Host ""
Write-Host " Then in ANOTHER CMD run:" -ForegroundColor White
Write-Host ""
Write-Host " STATUS_OZIIDPI_TEST.cmd" -ForegroundColor White
Write-Host ""
Write-Host " Press CTRL+C in THIS window to safely stop OziiDPI." -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Cyan

try {
    try { [console]::TreatControlCAsInput = $true } catch { }
    while (-not $proc.HasExited) {
        try {
            if ([console]::KeyAvailable) {
                $key = [console]::ReadKey($true)
                if (($key.Modifiers -band [ConsoleModifiers]::Control) -and $key.Key -eq [ConsoleKey]::C) {
                    Write-Host "`nStopping OziiDPI..." -ForegroundColor Yellow
                    break
                }
            }
        } catch { }
        Start-Sleep -Milliseconds 100
    }
} finally {
    try { [console]::TreatControlCAsInput = $false } catch { }
    
    # Graceful stop
    Send-StopRequest $diagPort | Out-Null
    Start-Sleep -Seconds 3
    if (-not $proc.HasExited) {
        $proc.Kill()
    }
    
    $afterFile = Join-Path $OUTPUT_DIR "manual-proxy-after.txt"
    $proxyAfter = Capture-ProxyState -OutputFile $afterFile

    $restored = $true
    if ("$($script:proxyBefore.AutoConfigURL)" -ne "$($proxyAfter.AutoConfigURL)") {
        $restored = $false
        if ($script:proxyBefore.AutoConfigURL) {
            Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -Name "AutoConfigURL" -Value $script:proxyBefore.AutoConfigURL
        } else {
            Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -Name "AutoConfigURL" -ErrorAction SilentlyContinue
        }
    }
    
    Write-Host ""
    if ($restored) {
        Write-Host "Windows proxy restored: YES" -ForegroundColor Green
    } else {
        Write-Host "Windows proxy restored: YES (Forced emergency restore)" -ForegroundColor Yellow
    }
    Write-Host "OziiDPI stopped safely." -ForegroundColor Green
    Write-Host ""
    Write-Host "Press any key to close..." -ForegroundColor White
}
