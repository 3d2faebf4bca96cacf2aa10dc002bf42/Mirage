# Bridge-first voice test (docs/verification.md V-23/V-25).
#
# The failing bridge measurements always had a peer transmitting BEFORE the bridge
# connected. The server does not deliver voice to clients that join after a speaker
# starts, so this script inverts the order: connect the bridge first, then start the
# speaker, and check that received voice is counted.
#
# Usage: pwsh scripts/bridge-first.ps1 [-Tone false] [-MeasureSeconds 16]

param(
    [string]$Tone = 'false',
    [int]$MeasureSeconds = 16,
    [string]$Nickname = 'bridge-first'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$env:ALLOW_PRIVATE_TARGETS = '1'
$client = Join-Path $PSScriptRoot '..\target\debug\ts6-cli.exe'

# 1. Bridge client connects and joins the default channel.
$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ct = [System.Threading.CancellationToken]::None
$ws.ConnectAsync([Uri]'ws://127.0.0.1:8787/ws', $ct).Wait()

$buffer = New-Object byte[] 262144
$segment = [ArraySegment[byte]]::new($buffer)

function Receive-Message([int]$timeoutMs = 1000) {
    $task = $ws.ReceiveAsync($segment, $ct)
    if ($task.Wait($timeoutMs)) { return [System.Text.Encoding]::UTF8.GetString($buffer, 0, $task.Result.Count) }
    return $null
}
function Send-Json([string]$json) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).Wait()
}

$null = Receive-Message 5000
Send-Json (@{ op = 'connect'; address = '127.0.0.1:9987'; nickname = $Nickname } | ConvertTo-Json -Compress)
$connected = $false
$deadline = (Get-Date).AddSeconds(30)
while (-not $connected -and (Get-Date) -lt $deadline) {
    $msg = Receive-Message 2000
    if ($msg -and $msg -match '"ev":"status"' -and $msg -match '"connected":true') { $connected = $true }
}
if (-not $connected) { Write-Host 'BRIDGE FAILED TO CONNECT'; $ws.Dispose(); exit 1 }
Write-Host 'bridge connected first'

Send-Json (@{ op = 'audio'; enabled = $true; tone = ($Tone -eq 'true'); ptt = ''; input_device = ''; output_device = '' } | ConvertTo-Json -Compress)
Start-Sleep -Seconds 2

# 2. Only now the speaker joins.
$peerLog = Join-Path $env:TEMP 'ts6-bridgefirst-peer.log'
Remove-Item $peerLog -ErrorAction SilentlyContinue
$peer = Start-Process -FilePath $client -ArgumentList @(
    'audio-send', '127.0.0.1:9987', '12', '--name', 'bfspeaker', '--identity', (Join-Path $env:TEMP 'ts6-bfspeaker.json')
) -NoNewWindow -PassThru -RedirectStandardOutput $peerLog -RedirectStandardError "$peerLog.err"
Write-Host 'speaker launched after the bridge joined'

$baseline = $null
$last = 0
$stopAt = (Get-Date).AddSeconds($MeasureSeconds)
while ((Get-Date) -lt $stopAt) {
    $msg = Receive-Message 500
    if (-not $msg) { continue }
    if ($msg -match '"ev":"audio"') {
        try { $parsed = $msg | ConvertFrom-Json } catch { continue }
        if ($null -eq $baseline) { $baseline = $parsed.rx_packets }
        $last = $parsed.rx_packets
    }
}

$delta = $last - $baseline
Write-Host ("RESULT baseline_rx={0} last_rx={1} delta={2} -> {3}" -f $baseline, $last, $delta, $(if ($delta -gt 0) { 'BRIDGE RX OK' } else { 'BRIDGE RX FAILED' }))
$peer | Wait-Process -Timeout 20 -ErrorAction SilentlyContinue
$ws.Dispose()
