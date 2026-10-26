# Network tests (plan §18.6, Stage 0 WebView2 spike)

Two layers of evidence, because each misses things the other sees:

| Tool | Sees | Misses | Needs admin |
|---|---|---|---|
| `tools/netmon.ps1` (polling `Get-NetTCPConnection` over the process tree) | which process, remote IP, port | connections shorter than the poll interval; UDP destinations; host names | no |
| Chromium NetLog (`TSR_STAGE0_NETLOG=<file>`, debug builds only) | every request Chromium's network service makes, with host names | traffic from code outside Chromium's network stack | no |
| Sysmon Event ID 3 / WFP audit | every connection, every process | content | yes |
| Packet capture (`pktmon` or Wireshark) | TLS SNI host names, DNS queries | — | yes |

Stage 0 exit and S1 (Stage 7) require the last two. The first two are the quick daily check.

## Quick check (no admin, ~10 minutes)

```powershell
cargo build -p tessara --features custom-protocol
$env:TSR_STAGE0_NETLOG = "$env:TEMP\tsr-netlog.json"      # optional, debug builds only
./tools/netmon.ps1 -Exe "$PWD\target\debug\tessara.exe" -Minutes 10 -Out "$PWD\bench\results\netmon.csv"
```

Exit code 1 and a table if any non-loopback TCP connection was seen. Local addresses are not
recorded (only ports), so the CSV holds no identifier of the machine.

## Two-hour run (Stage 0 requirement)

1. Elevated PowerShell, start a capture filtered to nothing (capture all, filter later):
   `pktmon start --capture --pkt-size 0 -f "$env:TEMP\tsr-2h.etl"`
2. In a normal PowerShell: `./tools/netmon.ps1 -Exe <tessara.exe> -Minutes 120 -Out bench/results/netmon-2h.csv`
3. During the run, exercise every feature that exists at the time (today: the pill window only;
   later: capture, OCR, model load, search, Ask, draft).
4. Elevated: `pktmon stop`, then `pktmon etl2pcap "$env:TEMP\tsr-2h.etl" -o "$env:TEMP\tsr-2h.pcapng"`.
5. Open in Wireshark, filter `tls.handshake.extensions_server_name || dns` and list host names.
   Match timestamps to the netmon CSV to attribute each to a process.
6. Record the result (host names, counts, which process) in `docs/stage0/results.md`. Do not commit
   the capture: it contains the machine's addresses.

## 24-hour run (Stage 7, S1)

As above for 24 h with normal use, plus Sysmon (`Event ID 3` filtered on `tessara.exe`,
`llama-server.exe` and `msedgewebview2.exe` whose parent chain leads to Tessara). A user-approved
model download or update check, if performed, is logged separately and excluded.

## Results so far (2026-10-26, polling + NetLog)

| Run | Duration | Non-loopback TCP | Notes |
|---|---|---|---|
| 1 | 10 min | 1 connection: `msedgewebview2.exe` → `2603:1046:c04:1404::2:443` at ~+1 min, closed ~+3 min | NetLog (separate run) showed `wpad:80` lookups |
| 2 | 5 min | 1 connection → `2603:1046:c04:1017::2:443` | with `--no-proxy-server`; NetLog: no WPAD, no record of this connection |
| 3 | 4 min | 1 connection → `2603:1046:c04:1422::2:443` | DNS cache had no entry for the address |

`tessara.exe` itself made no connections. The WebView2 connection is the open item in ADR-0002.
