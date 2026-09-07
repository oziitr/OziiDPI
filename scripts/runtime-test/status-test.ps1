# status-test.ps1
. "$PSScriptRoot\common.ps1"

$rtInfo = Get-RuntimeInfo
if ($rtInfo) {
    Write-Host "========================================"
    Write-Host " RUNNING OZII DPI STATUS"
    Write-Host "========================================"
    Write-Host " State:        CONNECTED"
    Write-Host " Mode:         $($rtInfo.mode)"
    Write-Host " Engine port:  $($rtInfo.engine_port)"
    Write-Host " Adapter port: $($rtInfo.adapter_port)"
    Write-Host " PAC URL:      $($rtInfo.pac_url)"
    Write-Host " Diag port:    $($rtInfo.diagnostics_port)"
    Write-Host ""
    $diag = Query-Diagnostics $rtInfo.diagnostics_port
    if ($diag) {
        $obj = $diag | ConvertFrom-Json
        Write-Host " Discord forwarded:      $($obj.discord_forwarded)"
        Write-Host " Discord failed:         $($obj.discord_failed)"
        Write-Host " Non-Discord rejected:   $($obj.non_discord_rejected)"
        Write-Host " Non-Discord forwarded:  $($obj.non_discord_forwarded)"
        Write-Host ""
        if ($obj.non_discord_forwarded -eq 0) {
            Write-Pass "Non-Discord forwarded = 0"
        } else {
            Write-Fail "Non-Discord forwarded = $($obj.non_discord_forwarded)"
        }
    } else {
        Write-Host "Could not retrieve diagnostics data."
    }
} else {
    Write-Host "No running OziiDPI test backend found."
}
