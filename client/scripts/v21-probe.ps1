# Drives the bridge over its WebSocket to reproduce docs/verification.md V-21.
# Usage: pwsh scripts/v21-probe.ps1 -Tone true|false
param(
    [string]$Tone = 'true',
    [int]$PeerSeconds = 8,
    [string]$Nickname = 'bridgeA'
)
$toneEnabled = $Tone -eq 'true'
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ct = [System.Threading.CancellationToken]::None
$ws.ConnectAsync([Uri]'ws://127.0.0.1:8787/ws', $ct).Wait()
Write-Host "ws state: $($ws.State)"

$buffer = New-Object byte[] 262144
$segment = [ArraySegment[byte]]::new($buffer)

function Receive-Message([int]$timeoutMs = 4000) {
    $task = $ws.ReceiveAsync($segment, $ct)
    if ($task.Wait($timeoutMs)) {
        return [System.Text.Encoding]::UTF8.GetString($buffer, 0, $task.Result.Count)
    }
    return $null
}

function Send-Json([string]$json) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).Wait()
}

# hello
$hello = Receive-Message 5000
Write-Host ("hello: " + $hello.Substring(0, [Math]::Min(80, $hello.Length)))

Send-Json (@{ op = 'connect'; address = '127.0.0.1:9987'; nickname = $Nickname } | ConvertTo-Json -Compress)

$connected = $false
$deadline = (Get-Date).AddSeconds(20)
while (-not $connected -and (Get-Date) -lt $deadline) {
    $msg = Receive-Message 3000
    if (-not $msg) { continue }
    if ($msg -match '"ev":"status"') {
        Write-Host ("status: " + $msg)
        if ($msg -match '"connected":true') { $connected = $true }
    }
}
if (-not $connected) { Write-Host 'FAILED TO CONNECT'; $ws.Dispose(); exit 1 }

Send-Json (@{ op = 'audio'; enabled = $true; tone = $toneEnabled; ptt = ''; input_device = ''; output_device = '' } | ConvertTo-Json -Compress)
Write-Host "audio enabled (tone=$toneEnabled)"

$env:ALLOW_PRIVATE_TARGETS = '1'
Start-Sleep -Milliseconds 1500

$client = Join-Path $PSScriptRoot '..\target\debug\ts6-cli.exe'
$identity = Join-Path $env:TEMP 'ts6-v21peer.json'
$peer = Start-Process -FilePath $client -ArgumentList @(
    'audio-send', '127.0.0.1:9987', "$PeerSeconds",
    '--name', 'v21peer', '--identity', $identity
) -NoNewWindow -PassThru
Write-Host "peer pid: $($peer.Id)"

$lastAudio = $null
$stopAt = (Get-Date).AddSeconds($PeerSeconds + 4)
while ((Get-Date) -lt $stopAt) {
    $msg = Receive-Message 2000
    if (-not $msg) { continue }
    if ($msg -match '"ev":"audio"') { $lastAudio = $msg }
}

if ($lastAudio) {
    Write-Host ('last audio event: ' + $lastAudio)
    $parsed = $lastAudio | ConvertFrom-Json
    Write-Host ("RESULT tone=$toneEnabled rx_packets=$($parsed.rx_packets) tx_frames=$($parsed.tx_frames) speaking=$($parsed.speaking -join ',')")
} else {
    Write-Host "RESULT tone=$toneEnabled rx_packets=<no audio event>"
}

$peer | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
$ws.Dispose()
