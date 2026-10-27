# Canary disk scan (plan §18.5, MVP criterion S2): no plaintext copy of captured content on disk.
#
#   .\tools\canary-scan.ps1 [-Seconds 60]
#
# 1. Seeds a throwaway encrypted store (tsr-eval seed --canary) whose items, title, fact and
#    summary carry a fresh random canary token.
# 2. Runs tessara.exe on it (TSR_DATA_DIR) with the expanded panel open for -Seconds, so the
#    canary goes through rollups, the brief, IPC and the WebView.
# 3. Stops only that process, then scans for the token as UTF-8/ASCII and UTF-16LE in:
#    the data folder, the WebView2 profile, %TEMP% (files changed during the run),
#    %LOCALAPPDATA%\CrashDumps and WER report folders (changed during the run).
# Writes evals/results/canary_scan.json. Exit 0 = no hits, 1 = hits, 2 = could not run.
#
# Not covered (documented in docs/privacy.md): pagefile.sys and hiberfil.sys (need elevation;
# memory pages may be written there by Windows), and a full-disk scan.

param([int]$Seconds = 60)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target\debug"
$token = "TSRCANARY" + ([guid]::NewGuid().ToString("N").Substring(0, 16).ToUpper())
$data = Join-Path $env:TEMP ("tessara-canary-" + [guid]::NewGuid().ToString("N"))
$start = Get-Date

& (Join-Path $bin "tsr-eval.exe") seed --dest $data --canary $token | Out-Host
if ($LASTEXITCODE -ne 0) { Write-Error "seed failed"; exit 2 }

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = Join-Path $bin "tessara.exe"
$psi.UseShellExecute = $false
$psi.EnvironmentVariables["TSR_DATA_DIR"] = $data
$psi.EnvironmentVariables["TSR_START_PANEL"] = "expanded"
$app = [System.Diagnostics.Process]::Start($psi)
Start-Sleep -Seconds $Seconds
if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
$app.WaitForExit(10000) | Out-Null
Start-Sleep -Seconds 2

# Positive control: a file holding the token (UTF-16) must be found, or the scan proves nothing.
$control = Join-Path $data "scan-control.txt"
[IO.File]::WriteAllText($control, "control $token", [Text.Encoding]::Unicode)
$latin1 = [Text.Encoding]::GetEncoding(28591)
$utf16 = ($token.ToCharArray() | ForEach-Object { "$_`0" }) -join ""
$roots = @(
    @{ path = $data; since = $null },
    @{ path = (Join-Path $env:LOCALAPPDATA "app.tessara.desktop"); since = $null },
    @{ path = $env:TEMP; since = $start },
    @{ path = (Join-Path $env:LOCALAPPDATA "CrashDumps"); since = $start },
    @{ path = (Join-Path $env:LOCALAPPDATA "Microsoft\Windows\WER"); since = $start }
)
$controlFound = $false; $hits = @(); $scanned = 0; $skipped = 0; $bytes = 0
foreach ($r in $roots) {
    if (-not (Test-Path $r.path)) { continue }
    foreach ($f in Get-ChildItem -LiteralPath $r.path -Recurse -File -Force -ErrorAction SilentlyContinue) {
        if ($r.since -and $f.LastWriteTime -lt $r.since) { continue }
        if ($f.Length -gt 512MB) { $skipped++; continue }
        try {
            $s = $latin1.GetString([IO.File]::ReadAllBytes($f.FullName))
            $scanned++; $bytes += $f.Length
            if ($s.IndexOf($token, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or $s.IndexOf($utf16, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                if ($f.FullName -eq $control) { $controlFound = $true; continue }
                $hits += $f.FullName.Replace($env:USERPROFILE, "%USERPROFILE%")
            }
        } catch { $skipped++ }
    }
}
Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue

$report = [ordered]@{
    measured_at = (Get-Date).ToString("s")
    run_seconds = $Seconds
    files_scanned = $scanned
    bytes_scanned = $bytes
    files_skipped = $skipped
    roots = @("data folder", "WebView2 profile", "%TEMP% (changed during run)", "CrashDumps (changed during run)", "WER (changed during run)")
    control_found = $controlFound
    plaintext_hits = $hits
    pass = ($hits.Count -eq 0 -and $scanned -gt 0 -and $controlFound)
}
$out = Join-Path $root "evals\results\canary_scan.json"
$report | ConvertTo-Json -Depth 4 | Out-File -Encoding utf8 $out
Get-Content $out | Out-Host
if ($scanned -eq 0 -or -not $controlFound) { exit 2 }
if ($hits.Count -gt 0) { exit 1 }
exit 0
