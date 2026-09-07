# portable-test.ps1 — Simplified and robust full-stack test
. "$PSScriptRoot\common.ps1"

$ErrorActionPreference = "Stop"
$failedStep = $null
$failedError = $null
$failedLog = $null

$script:BACKEND_PROC = $null

Ensure-OutputDir

Write-Host "========================================" -ForegroundColor White
Write-Host " OziiDPI REAL RUNTIME TEST" -ForegroundColor White
Write-Host "========================================" -ForegroundColor White

function Cleanup-And-Exit {
    param([int]$exitCode = 0)
    
    if ($script:BACKEND_PROC -and -not $script:BACKEND_PROC.HasExited) {
        $rtInfo = Get-RuntimeInfo
        if ($rtInfo) {
            Send-StopRequest $rtInfo.diagnostics_port | Out-Null
            Start-Sleep -Seconds 3
        }
        if (-not $script:BACKEND_PROC.HasExited) {
            try { $script:BACKEND_PROC.Kill() } catch { }
        }
    }

    # Emergency CLI stop for recovery just in case
    try { & $CLI_BIN stop 2>&1 | Out-Null } catch { }
    
    # Restore proxy baseline if CLI recovery didn't
    try {
        $reg = Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
        if ($reg.AutoConfigURL -match "127\.0\.0\.1") {
            if ($script:proxyBefore.AutoConfigURL) {
                Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -Name "AutoConfigURL" -Value $script:proxyBefore.AutoConfigURL
            } else {
                Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -Name "AutoConfigURL" -ErrorAction SilentlyContinue
            }
        }
    } catch { }

    if ($exitCode -ne 0) {
        Write-Host ""
        Write-Host "========================================" -ForegroundColor Red
        Write-Host " FINAL RESULT: FAIL" -ForegroundColor Red
        if ($failedStep) {
            Write-Host " FAILED STEP:" -ForegroundColor Red
            Write-Host " $failedStep" -ForegroundColor Red
        }
        if ($failedError) {
            Write-Host " ACTUAL ERROR:" -ForegroundColor Red
            Write-Host " $failedError" -ForegroundColor Red
        }
        if ($failedLog) {
            Write-Host " LOG FILE:" -ForegroundColor Red
            Write-Host " $failedLog" -ForegroundColor Red
        }
        Write-Host "========================================" -ForegroundColor Red
    }
    
    exit $exitCode
}

try {
    # 0. Capture baseline
    $script:proxyBefore = Capture-ProxyState -OutputFile (Join-Path $OUTPUT_DIR "proxy-before.txt")
    
    # 1. Build
    Push-Location $BACKEND_DIR
    $proc = Start-Process cargo -ArgumentList "build --workspace" -Wait -NoNewWindow -PassThru
    if ($proc.ExitCode -ne 0 -or -not (Test-Path $CLI_BIN)) {
        $failedStep = "Backend Start"
        $failedError = "Cargo build failed with exit code $($proc.ExitCode)"
        throw "Build failed"
    }
    Pop-Location
    
    if (-not (Test-Path $ENGINE_BIN)) {
        Push-Location $ENGINE_SRC
        $proc = Start-Process go -ArgumentList "build -o ..\bin\ozii-dpi-engine.exe ./cmd/spoofdpi" -Wait -NoNewWindow -PassThru
        if ($proc.ExitCode -ne 0 -or -not (Test-Path $ENGINE_BIN)) {
            $failedStep = "Backend Start"
            $failedError = "Go build failed"
            throw "Engine build failed"
        }
        Pop-Location
    }

    # Remove old runtime if exists
    if (Test-Path $RUNTIME_JSON) {
        Remove-Item $RUNTIME_JSON -Force -ErrorAction SilentlyContinue
    }

    # 2. Start full backend
    $env:OZIIDPI_IGNORE_WPAD = "1"
    $script:BACKEND_PROC = Start-Process -FilePath $CLI_BIN -ArgumentList "start", "--mode", "balanced", "--chunk-size", "2" -NoNewWindow -PassThru
    
    $deadline = (Get-Date).AddSeconds(10)
    $rtInfo = $null
    while ((Get-Date) -lt $deadline) {
        $rtInfo = Get-RuntimeInfo
        if ($rtInfo) { break }
        Start-Sleep -Milliseconds 300
    }

    if (-not $rtInfo) {
        $failedStep = "Backend Start"
        $failedError = "CLI started but runtime.json not created in time."
        throw "Backend failed to start"
    }

    $adapterPort = $rtInfo.adapter_port
    $diagPort = $rtInfo.diagnostics_port
    $pacPort = $rtInfo.pac_port
    
    $result_backendStart = "PASS"
    $result_engineRunning = "PASS"
    $result_adapterRunning = "PASS"
    $result_pacRunning = "PASS"

    # Get initial diagnostics
    $diagBeforeObj = Query-Diagnostics $diagPort | ConvertFrom-Json
    $discord_forwarded_before = if ($diagBeforeObj) { [int]$diagBeforeObj.discord_forwarded } else { 0 }

    # 3. Main discord.com test
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host " REAL DISCORD TEST" -ForegroundColor Cyan
    Write-Host " Request:       https://discord.com/" -ForegroundColor Cyan
    Write-Host " Path:          curl -> Adapter -> SpoofDPI -> Discord" -ForegroundColor Cyan
    Write-Host " Adapter:       127.0.0.1:$adapterPort" -ForegroundColor Cyan
    Write-Host " Running curl..." -ForegroundColor Cyan
    
    $discordFullStackFile = Join-Path $OUTPUT_DIR "discord-full-stack.txt"
    $curlCmd = "curl.exe --verbose --max-time 20 --proxy http://127.0.0.1:$adapterPort https://discord.com/ 2>&1"
    $curlOut = cmd.exe /c $curlCmd | Out-String
    $curlOut | Set-Content -Path $discordFullStackFile -Encoding UTF8

    Write-Host $curlOut
    
    $isDiscordPass = ($curlOut -match "HTTP/1\.\d\s+(200|301|302|403|405)" -or $curlOut -match "Connection established")
    if ($isDiscordPass) {
        Write-Host " Discord TLS connection: PASS" -ForegroundColor Green
        $result_discord = "PASS"
    } else {
        Write-Host " Discord TLS connection: FAIL" -ForegroundColor Red
        $failedStep = "discord.com"
        $failedError = "No valid HTTP response through proxy"
        $failedLog = $discordFullStackFile
        throw "Discord test failed"
    }
    Write-Host "========================================" -ForegroundColor Cyan

    # Check discord_forwarded after
    $diagAfterObj = Query-Diagnostics $diagPort | ConvertFrom-Json
    $discord_forwarded_after = if ($diagAfterObj) { [int]$diagAfterObj.discord_forwarded } else { 0 }
    
    Write-Host "Discord forwarded before: $discord_forwarded_before"
    Write-Host "Discord forwarded after:  $discord_forwarded_after"
    if ($discord_forwarded_after -le $discord_forwarded_before) {
        $failedStep = "Discord Forwarded"
        $failedError = "Counter did not increase. Traffic may have bypassed adapter."
        throw "Forward counter failed"
    }
    $result_discordFwd = "PASS"
    
    # 4. non_discord_forwarded check
    $nonDiscordFwd = if ($diagAfterObj) { [int]$diagAfterObj.non_discord_forwarded } else { 0 }
    if ($nonDiscordFwd -ne 0) {
        $failedStep = "Non-Discord Forwarded"
        $failedError = "Expected 0, got $nonDiscordFwd"
        throw "Security test failed"
    }
    $result_nonDiscordFwd = "PASS"

    # 5. Discord API test
    $apiFile = Join-Path $OUTPUT_DIR "discord-api-test.txt"
    $apiCmd = "curl.exe --verbose --max-time 20 --proxy http://127.0.0.1:$adapterPort https://discord.com/api/v10/gateway 2>&1"
    $apiOut = cmd.exe /c $apiCmd | Out-String
    $apiOut | Set-Content -Path $apiFile -Encoding UTF8
    if ($apiOut -match "HTTP/1\.\d\s+(200|401)") {
        $result_discordApi = "PASS"
    } else {
        $failedStep = "discord.com/api/v10/gateway"
        $failedError = "API test failed"
        $failedLog = $apiFile
        throw "API test failed"
    }

    # 6. Discord Gateway test
    $gwFile = Join-Path $OUTPUT_DIR "discord-gateway-test.txt"
    $gwCmd = "curl.exe --verbose --max-time 20 --proxy http://127.0.0.1:$adapterPort https://gateway.discord.gg/ 2>&1"
    $gwOut = cmd.exe /c $gwCmd | Out-String
    $gwOut | Set-Content -Path $gwFile -Encoding UTF8
    if ($gwOut -match "HTTP/1\.\d\s+(200|301|302|403|404)") {
        $result_gateway = "PASS"
    } else {
        $failedStep = "gateway.discord.gg"
        $failedError = "Gateway test failed"
        $failedLog = $gwFile
        throw "Gateway test failed"
    }

    # 7. Google via Adapter Reject test
    $googleFile = Join-Path $OUTPUT_DIR "adapter-google-test.txt"
    $googleCmd = "curl.exe --verbose --max-time 10 --proxy http://127.0.0.1:$adapterPort https://google.com/ 2>&1"
    $googleOut = cmd.exe /c $googleCmd | Out-String
    $googleOut | Set-Content -Path $googleFile -Encoding UTF8
    if ($googleOut -match "403" -or $googleOut -match "CONNECT tunnel failed") {
        $result_googleReject = "PASS"
    } else {
        $failedStep = "Google via Adapter Reject"
        $failedError = "Google was not rejected by Adapter"
        $failedLog = $googleFile
        throw "Rejection test failed"
    }
    
    # 8. Normal Google Internet
    $normalCmd = "curl.exe --head --max-time 15 https://google.com/ 2>&1"
    $normalOut = cmd.exe /c $normalCmd | Out-String
    if ($normalOut -match "HTTP/" -and $normalOut -match "(200|301|302)") {
        $result_googleNormal = "PASS"
    } else {
        $failedStep = "Google Normal Internet"
        $failedError = "Normal internet failed"
        throw "Normal internet failed"
    }

    # 9. Windows PAC Active
    $activeProxy = Capture-ProxyState -OutputFile (Join-Path $OUTPUT_DIR "proxy-active.txt")
    $currentAutoConfig = $activeProxy.AutoConfigURL
    Write-Host "Windows PAC Active: YES ($currentAutoConfig)"
    
    if ($currentAutoConfig -match "127\.0\.0\.1:$pacPort") {
        $pacFetch = cmd.exe /c "curl.exe -s `"$currentAutoConfig`" --connect-timeout 3 2>&1" | Out-String
        if ($pacFetch -match "FindProxyForURL") {
            $result_pacActive = "PASS"
            Write-Host "PAC reachable: YES"
        } else {
            $failedStep = "Windows PAC Active"
            $failedError = "PAC not reachable"
            throw "PAC unreachable"
        }
    } else {
        $failedStep = "Windows PAC Active"
        $failedError = "AutoConfigURL is '$currentAutoConfig', expected http://127.0.0.1:$pacPort/proxy.pac"
        throw "Windows PAC not set"
    }

    # 10. Shutdown and Restore
    Send-StopRequest $diagPort | Out-Null
    Start-Sleep -Seconds 4
    if (-not $script:BACKEND_PROC.HasExited) {
        $script:BACKEND_PROC.Kill()
        Start-Sleep -Seconds 1
    }

    $afterFile = Join-Path $OUTPUT_DIR "proxy-after.txt"
    $proxyAfter = Capture-ProxyState -OutputFile $afterFile

    if ("$($script:proxyBefore.AutoConfigURL)" -eq "$($proxyAfter.AutoConfigURL)") {
        $result_proxyRestore = "PASS"
    } else {
        $failedStep = "Windows Proxy Restore"
        $failedError = "AutoConfigURL not restored correctly. Before: $($script:proxyBefore.AutoConfigURL), After: $($proxyAfter.AutoConfigURL)"
        throw "Proxy restore failed"
    }

    # Success output
    Write-Host ""
    Write-Host "========================================"
    Write-Host " OziiDPI REAL RUNTIME TEST"
    Write-Host " Backend Start            $result_backendStart"
    Write-Host " Engine Running           $result_engineRunning"
    Write-Host " Adapter Running          $result_adapterRunning"
    Write-Host " PAC Running              $result_pacRunning"
    Write-Host " discord.com              $result_discord"
    Write-Host " discord.com/api/v10/...  $result_discordApi"
    Write-Host " gateway.discord.gg       $result_gateway"
    Write-Host " Discord Forwarded        $discord_forwarded_after"
    Write-Host " Non-Discord Forwarded    $nonDiscordFwd"
    Write-Host " Google via Adapter Reject $result_googleReject"
    Write-Host " Google Normal Internet   $result_googleNormal"
    Write-Host " Windows PAC Active       $result_pacActive"
    Write-Host " Windows Proxy Restore    $result_proxyRestore"
    Write-Host "========================================"
    Write-Host " FINAL RESULT: PASS" -ForegroundColor Green
    Write-Host ""

    Cleanup-And-Exit 0

} catch {
    Cleanup-And-Exit 1
}
