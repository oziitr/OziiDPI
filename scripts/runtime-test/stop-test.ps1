# stop-test.ps1
. "$PSScriptRoot\common.ps1"

$rtInfo = Get-RuntimeInfo
if ($rtInfo) {
    Write-Host "Sending stop request to backend at port $($rtInfo.diagnostics_port)..."
    $res = Send-StopRequest $rtInfo.diagnostics_port
    if ($res) {
        Write-Host "Stop request sent. Backend should exit."
    } else {
        Write-Host "Could not reach backend. Trying CLI stop..."
        & $CLI_BIN stop
    }
} else {
    Write-Host "No runtime found. Trying CLI stop for recovery..."
    & $CLI_BIN stop
}
