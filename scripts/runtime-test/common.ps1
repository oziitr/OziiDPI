# common.ps1 — Shared constants and helper functions for OziiDPI runtime test scripts

$ErrorActionPreference = "Stop"

# Paths
$script:ROOT_DIR      = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$script:BACKEND_DIR   = Join-Path $ROOT_DIR "backend"
$script:ENGINE_SRC    = Join-Path $ROOT_DIR "engine\SpoofDPI"
$script:ENGINE_BIN    = Join-Path $ROOT_DIR "engine\bin\ozii-dpi-engine.exe"
$script:CLI_BIN       = Join-Path $BACKEND_DIR "target\debug\ozii-cli.exe"
$script:OUTPUT_DIR    = Join-Path $ROOT_DIR "runtime-test-output"
$script:RUNTIME_JSON  = Join-Path $env:LOCALAPPDATA "OziiDPI\runtime.json"

# Ports for isolated testing (steps C-F, before full backend)
$script:TEST_ENGINE_PORT  = 18080
$script:TEST_ADAPTER_PORT = 18081
$script:TEST_PAC_PORT     = 18082
$script:TEST_DIAG_PORT    = 18083

function Ensure-OutputDir {
    if (-not (Test-Path $OUTPUT_DIR)) {
        New-Item -ItemType Directory -Path $OUTPUT_DIR -Force | Out-Null
    }
}

function Write-Step {
    param([string]$StepNum, [string]$Total, [string]$Label)
    Write-Host ""
    Write-Host "[$StepNum/$Total] $Label" -ForegroundColor Cyan
}

function Write-Pass {
    param([string]$Label)
    Write-Host "[PASS] $Label" -ForegroundColor Green
}

function Write-Fail {
    param([string]$Label, [string]$Detail = "")
    Write-Host "[FAIL] $Label" -ForegroundColor Red
    if ($Detail) { Write-Host "       $Detail" -ForegroundColor Red }
}

function Write-Warn {
    param([string]$Label)
    Write-Host "[WARN] $Label" -ForegroundColor Yellow
}

function Capture-ProxyState {
    param([string]$OutputFile)
    $reg = Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
    $state = @{
        ProxyEnable    = $reg.ProxyEnable
        ProxyServer    = $reg.ProxyServer
        ProxyOverride  = $reg.ProxyOverride
        AutoConfigURL  = $reg.AutoConfigURL
    }
    $state | ConvertTo-Json | Set-Content -Path $OutputFile -Encoding UTF8
    return $state
}

function Wait-ForPort {
    param([int]$Port, [int]$TimeoutSeconds = 10)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $tcp = New-Object System.Net.Sockets.TcpClient
            $tcp.Connect("127.0.0.1", $Port)
            $tcp.Close()
            return $true
        } catch {
            Start-Sleep -Milliseconds 200
        }
    }
    return $false
}

function Stop-ProcessByPort {
    param([int]$Port)
    try {
        $connections = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
        if ($connections) {
            foreach ($conn in $connections) {
                Stop-Process -Id $conn.OwningProcess -Force -ErrorAction SilentlyContinue
            }
        }
    } catch { }
}

function Get-RuntimeInfo {
    if (Test-Path $RUNTIME_JSON) {
        try {
            return Get-Content $RUNTIME_JSON -Raw | ConvertFrom-Json
        } catch { return $null }
    }
    return $null
}

function Query-Diagnostics {
    param([int]$Port)
    try {
        $result = cmd.exe /c "curl.exe -s http://127.0.0.1:$Port/ --connect-timeout 3 2>&1"
        return $result | Out-String
    } catch { return $null }
}

function Send-StopRequest {
    param([int]$Port)
    try {
        $result = cmd.exe /c "curl.exe -s -X POST http://127.0.0.1:$Port/stop --connect-timeout 3 2>&1"
        return ($result | Out-String).Trim()
    } catch { return $null }
}
