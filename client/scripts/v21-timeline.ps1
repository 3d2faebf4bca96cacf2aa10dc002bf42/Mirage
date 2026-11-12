# Measures WHEN the bridge starts counting received voice (docs/verification.md V-21).
# Keeps the WebSocket open for the whole window so the measurement is not affected
# by the client disconnecting.
param(
    [string]$Tone = 'true',
    [int]$PeerSeconds = 8,
    [int]$WindowSeconds = 40,
    [string]$Nickname = 'bridgeT'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ct = [System.Threading.CancellationToken]::None
$ws.ConnectAsync([Uri]'ws://127.0.0.1:8787/ws', $ct).Wait()

$buffer = New-Object byte[] 262144
$segment = [ArraySegment[byte]]::new($buffer)

function Receive-Message([int]$timeoutMs = 1000) {
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

$null = Receive-Message 5000   # hello
Send-Json (@{ op = 'connect'; address = '127.0.0.1:9987'; nickname = $Nickname } | ConvertTo-Json -Compress)

$connected = $false
$deadline = (Get-Date).AddSeconds(20)
while (-not $connected -and (Get-Date) -lt $deadline) {
    $msg = Receive-Message 2000
    if ($msg -and $msg -match '"ev":"status"' -and $msg -match '"connected":true') { $connected = $true }
}
if (-not $connected) { Write-Host 'FAILED TO CONNECT'; $ws.Dispose(); exit 1 }

Send-Json (@{ op = 'audio'; enabled = $true; tone = ($Tone -eq 'true'); ptt = ''; input_device = ''; output_device = '' } | ConvertTo-Json -Compress)
$t0 = Get-Date
Write-Host ("audio enabled at t+0.0s (tone=$Tone)")

$env:ALLOW_PRIVATE_TARGETS = '1'
$client = Join-Path $PSScriptRoot '..\target\debug\ts6-cli.exe'
$identity = Join-Path $env:TEMP 'ts6-v21peer.json'
Start-Sleep -Milliseconds 1500
$peer = Start-Process -FilePath $client -ArgumentList @(
    'audio-send', '127.0.0.1:9987', "$PeerSeconds", '--name', 'v21peer', '--identity', $identity
) -NoNewWindow -PassThru
Write-Host ("peer started at t+{0:N1}s" -f ((Get-Date) - $t0).TotalSeconds)

$firstRx = $null
$baselineRx = $null
$lastRx = 0
$stopAt = $t0.AddSeconds($WindowSeconds)
while ((Get-Date) -lt $stopAt) {
    $msg = Receive-Message 500
    if (-not $msg) { continue }
    if ($msg -match '"ev":"audio"') {
        try { $parsed = $msg | ConvertFrom-Json } catch { continue }
        $elapsed = ((Get-Date) - $t0).TotalSeconds
        if ($null -eq $baselineRx) { $baselineRx = $parsed.rx_packets }
        if (($parsed.rx_packets - $baselineRx) -gt 0 -and -not $firstRx) {
            $firstRx = $elapsed
            Write-Host ("FIRST RX>0 at t+{0:N1}s (rx={1})" -f $elapsed, $parsed.rx_packets)
        }
        $lastRx = $parsed.rx_packets
        if ($parsed.rx_packets -gt 0) {
            Write-Host ("  t+{0:N1}s rx={1} tx={2} speaking={3}" -f $elapsed, $parsed.rx_packets, $parsed.tx_frames, ($parsed.speaking -join ','))
        }
    }
}

Write-Host ("RESULT tone={0} baseline_rx={1} last_rx={2} delta={3} first_delta_time={4}" -f `
    $Tone, $baselineRx, $lastRx, ($lastRx - $baselineRx),
    ($(if ($firstRx) { "t+$([Math]::Round($firstRx, 1))s" } else { 'never' })))
$peer | Wait-Process -Timeout 20 -ErrorAction SilentlyContinue
$ws.Dispose()
