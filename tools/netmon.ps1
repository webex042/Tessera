<#
.SYNOPSIS
  Watch a process tree for outbound network connections (plan section 18.6, Stage 0 WebView2 spike).

.DESCRIPTION
  Starts the given executable, then polls every -IntervalMs for TCP connections owned by the
  process or any descendant (e.g. msedgewebview2.exe children) and for UDP sockets they open.
  Any TCP remote address that is not loopback is recorded as a violation.

  LIMITATION: this is polling. A connection that opens and closes between two polls is missed,
  and UDP datagrams (e.g. DNS) show only as bound sockets, not destinations. For the formal
  24-hour test (section 18.6) use Sysmon Event ID 3 or a WFP audit plus a packet capture; see
  docs/stage0/network-test.md. This script is the quick, dependency-free first pass.

.EXAMPLE
  ./tools/netmon.ps1 -Exe target/debug/tessara.exe -Minutes 120 -Out bench/results/netmon-2h.csv
#>
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string[]] $Arguments = @(),
    [double] $Minutes = 10,
    [int] $IntervalMs = 250,
    [string] $Out = "netmon.csv"
)

$ErrorActionPreference = 'Stop'
# PowerShell 5.1 rejects an empty -ArgumentList, so only pass it when there is something to pass.
$startArgs = @{ FilePath = $Exe; PassThru = $true; WindowStyle = 'Normal' }
if ($Arguments.Count -gt 0) { $startArgs.ArgumentList = $Arguments }
$proc = Start-Process @startArgs
$rootPid = $proc.Id
$deadline = (Get-Date).AddMinutes($Minutes)
$seenTcp = @{}
$seenUdp = @{}
$seenProcs = @{}
$rows = New-Object System.Collections.Generic.List[object]
$polls = 0

function Get-Tree([int] $root) {
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
    $ids = New-Object System.Collections.Generic.HashSet[int]
    [void]$ids.Add($root)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($p in $all) {
            if ($ids.Contains([int]$p.ParentProcessId) -and -not $ids.Contains([int]$p.ProcessId)) {
                [void]$ids.Add([int]$p.ProcessId); $changed = $true
            }
        }
    }
    $all | Where-Object { $ids.Contains([int]$_.ProcessId) }
}

function Test-Loopback([string] $addr) {
    return $addr -eq '127.0.0.1' -or $addr -eq '::1' -or $addr -like '127.*' -or $addr -eq '0.0.0.0' -or $addr -eq '::'
}

Write-Host "Watching PID $rootPid ($Exe) for $Minutes min..."
while ((Get-Date) -lt $deadline -and -not $proc.HasExited) {
    $tree = @(Get-Tree $rootPid)
    foreach ($p in $tree) { $seenProcs["$($p.ProcessId)"] = $p.Name }
    $pids = $tree | ForEach-Object { [int]$_.ProcessId }
    $tcp = Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess }
    foreach ($c in $tcp) {
        $key = "$($c.OwningProcess)|$($c.RemoteAddress)|$($c.RemotePort)|$($c.State)"
        if (-not $seenTcp.ContainsKey($key)) {
            $seenTcp[$key] = $true
            $name = $seenProcs["$($c.OwningProcess)"]
            $isLoop = Test-Loopback $c.RemoteAddress
            # Name the destination while the DNS cache entry is still fresh (no admin needed).
            $dnsName = ''
            if (-not $isLoop) {
                $dnsName = (Get-DnsClientCache -ErrorAction SilentlyContinue | Where-Object { $_.Data -eq "$($c.RemoteAddress)" } |
                    Select-Object -ExpandProperty Entry -Unique) -join ' '
            }
            $rows.Add([pscustomobject]@{ time = (Get-Date).ToString('s'); proto = 'tcp'; pid = $c.OwningProcess; process = $name
                # Local address omitted on purpose: it is the user's own (often public) IP. Port only.
                local = ":$($c.LocalPort)"; remote = "$($c.RemoteAddress):$($c.RemotePort)"; state = "$($c.State)"
                loopback = $isLoop; dns = $dnsName })
        }
    }
    $udp = Get-NetUDPEndpoint -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess }
    foreach ($u in $udp) {
        $key = "$($u.OwningProcess)|$($u.LocalAddress)|$($u.LocalPort)"
        if (-not $seenUdp.ContainsKey($key)) {
            $seenUdp[$key] = $true
            $rows.Add([pscustomobject]@{ time = (Get-Date).ToString('s'); proto = 'udp-bind'; pid = $u.OwningProcess; process = $seenProcs["$($u.OwningProcess)"]
                local = ":$($u.LocalPort)"; remote = ''; state = 'bound'; loopback = (Test-Loopback $u.LocalAddress); dns = '' })
        }
    }
    $polls++
    Start-Sleep -Milliseconds $IntervalMs
}
if (-not $proc.HasExited) { Stop-Process -Id $rootPid -Force -ErrorAction SilentlyContinue; Start-Sleep 1 }
foreach ($p in (Get-Tree $rootPid)) { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }

$rows | Export-Csv -NoTypeInformation -Encoding UTF8 $Out
$violations = @($rows | Where-Object { $_.proto -eq 'tcp' -and -not $_.loopback })
$procNames = ($seenProcs.Values | Sort-Object -Unique) -join ', '
Write-Host "Polls: $polls  Processes seen: $procNames"
Write-Host "TCP endpoints: $($seenTcp.Count)  UDP sockets: $($seenUdp.Count)  Non-loopback TCP: $($violations.Count)"
if ($violations.Count -gt 0) {
    $violations | Format-Table time, process, remote, state, dns -AutoSize | Out-String | Write-Host
    exit 1
}
Write-Host "PASS (polling-level): no non-loopback TCP connections observed. Wrote $Out"
exit 0
