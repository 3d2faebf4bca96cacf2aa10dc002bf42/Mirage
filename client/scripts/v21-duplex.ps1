# Verifies duplex through the bridge with the peer ALREADY transmitting.
#
# The earlier probe started the peer and measured immediately, but the peer needs
# ~20-30 s to connect under load (RSA puzzle), so the window ended before any voice
# existed. This version starts the peer first and only then connects the bridge.
#
# Usage: pwsh scripts/v21-duplex.ps1 -Tone true -PeerSeconds 40 -MeasureSeconds 10
param(
    [string]$Tone = 'true',
    [string]$AudioEnabled = 'true',
    [int]$PeerSeconds = 40,
    [int]$MeasureSeconds = 10,
    [int]$PeerWarmupSeconds = 12,
    [string]$Nickname = 'duplex'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$env:ALLOW_PRIVATE_TARGETS = '1'
$client = Join-Path $PSScriptRoot '..\target\debug\ts6-cli.exe'
$identity = Join-Path $env:TEMP 'ts6-duplexpeer.json'

# 1. Peer transmits first, for long enough to cover the warmup plus the measurement.
$peerLog = Join-Path $env:TEMP 'ts6-duplexpeer.log'
Remove-Item $peerLog -ErrorAction SilentlyContinue
$peer = Start-Process -FilePath $client -ArgumentList @(
    'audio-send', '127.0.0.1:9987', "$PeerSeconds", '--name', 'duplexpeer', '--identity', $identity
) -NoNewWindow -PassThru -RedirectStandardOutput $peerLog -RedirectStandardError "$peerLog.err"
Write-Host "peer started (pid $($peer.Id)); waiting for it to actually transmit"

# The peer needs a while to connect (RSA puzzle under load): wait for its own
# confirmation line instead of guessing a warmup duration.
$warmDeadline = (Get-Date).AddSeconds($PeerWarmupSeconds + 60)
while ((Get-Date) -lt $warmDeadline) {
    if ((Test-Path $peerLog) -and (Select-String -Path $peerLog -Pattern 'sending 440 Hz' -Quiet)) { break }
    Start-Sleep -Milliseconds 500
}
if (-not ((Test-Path $peerLog) -and (Select-String -Path $peerLog -Pattern 'sending 440 Hz' -Quiet))) {
    Write-Host 'PEER NEVER STARTED SENDING'
    Get-Content $peerLog -ErrorAction SilentlyContinue | Select-Object -Last 5 | ForEach-Object { "  peer: $_" }
    $ws.Dispose()
    exit 1
}
Write-Host 'peer is transmitting'

# 2. Bridge client joins, enables audio, and counts received packets.
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
if (-not $connected) { Write-Host 'FAILED TO CONNECT'; $ws.Dispose(); exit 1 }
Write-Host 'bridge client connected'

Send-Json (@{ op = 'audio'; enabled = ($AudioEnabled -eq 'true'); tone = ($Tone -eq 'true'); ptt = ''; input_device = ''; output_device = '' } | ConvertTo-Json -Compress)
Write-Host "audio command sent (enabled=$AudioEnabled tone=$Tone); measuring for $MeasureSeconds s"

$baseline = $null
$last = 0
$stopAt = (Get-Date).AddSeconds($MeasureSeconds)
while ((Get-Date) -lt $stopAt) {
    $msg = Receive-Message 500
    if (-not $msg) { continue }
    if ($msg -match '"ev":"state"') {
        try { $state = $msg | ConvertFrom-Json } catch { $state = $null }
        if ($state) {
            $own = $state.snapshot.clients | Where-Object { $_.is_self } | Select-Object -First 1
            $others = ($state.snapshot.clients | Where-Object { -not $_.is_self } | ForEach-Object { "$($_.name)@ch$($_.channel_id)" }) -join ', '
            Write-Host ("state: self=$($own.name) ch=$($own.channel_id) | others: $others")
        }
    }
    if ($msg -match '"ev":"audio"') {
        try { $parsed = $msg | ConvertFrom-Json } catch { continue }
        if ($null -eq $baseline) { $baseline = $parsed.rx_packets }
        $last = $parsed.rx_packets
    }
}

$delta = $last - $baseline
$verdict = if ($delta -gt 0) { 'DUPLEX OK' } else { 'DUPLEX FAILED' }
Write-Host ("RESULT tone={0} baseline_rx={1} last_rx={2} delta={3} -> {4}" -f $Tone, $baseline, $last, $delta, $verdict)

$peer | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
$ws.Dispose()
