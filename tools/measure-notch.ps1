# Measures the notch on this machine (plan §13.4, Stage 3 exit criteria): frame pacing of the
# expand/collapse animations, input-to-visual latency, and focus behaviour.
#
#   .\tools\measure-notch.ps1 [-Cycles 10]
#
# 1. Seeds a throwaway synthetic store and starts tessara.exe on it (TSR_DATA_DIR) with
#    TSR_FRAME_LOG set, so the UI reports frame deltas and latencies (numbers only).
# 2. Checks the app takes no focus when it starts (pill state).
# 3. Presses the real global toggle hotkey (Ctrl+Alt+Space, or its fallback Ctrl+Alt+N when
#    another app owns it; synthesized key events) to open and close
#    the panel `-Cycles` times. After each close, the window that had focus before must have
#    it again.
# 4. Stops only the process it started; writes evals/results/notch_perf.json.
# Exit code 0 = every threshold met, 1 = a threshold missed, 2 = could not run.
#
# While it runs, the notch opens and closes at the top of the screen. Do not type meanwhile.

param([int]$Cycles = 10)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target\debug"

Add-Type -Namespace TsrMeasure -Name Win -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint pid);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool RegisterHotKey(System.IntPtr h, int id, uint mods, uint vk);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool UnregisterHotKey(System.IntPtr h, int id);
[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
public struct RECT { public int Left, Top, Right, Bottom; }
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern int GetWindowRgnBox(System.IntPtr h, out RECT r);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetDpiForWindow(System.IntPtr h);
'@
# The app falls back to Ctrl+Alt+N when another app owns Ctrl+Alt+Space; press the one it gets.
$toggleVk = 0x20
if ([TsrMeasure.Win]::RegisterHotKey([IntPtr]::Zero, 91, 0x4003, 0x20)) { [TsrMeasure.Win]::UnregisterHotKey([IntPtr]::Zero, 91) | Out-Null }
else { $toggleVk = 0x4E }
function Press-Toggle {
    # Ctrl+Alt+<toggle> down, then up in reverse order.
    foreach ($vk in 0x11, 0x12, $toggleVk) { [TsrMeasure.Win]::keybd_event([byte]$vk, 0, 0, [UIntPtr]::Zero) }
    foreach ($vk in $toggleVk, 0x12, 0x11) { [TsrMeasure.Win]::keybd_event([byte]$vk, 0, 2, [UIntPtr]::Zero) }
}
function Foreground-Pid {
    $p = 0; [TsrMeasure.Win]::GetWindowThreadProcessId([TsrMeasure.Win]::GetForegroundWindow(), [ref]$p) | Out-Null; $p
}
function Stats($xs) {
    if (-not $xs -or $xs.Count -eq 0) { return $null }
    $s = $xs | Sort-Object
    $pick = { param($q) $s[[math]::Min($s.Count - 1, [math]::Floor($q * $s.Count))] }
    [ordered]@{ n = $s.Count; p50 = [math]::Round((& $pick 0.5), 2); p95 = [math]::Round((& $pick 0.95), 2); max = [math]::Round($s[-1], 2) }
}

$data = Join-Path $env:TEMP ("tessara-measure-" + [guid]::NewGuid().ToString("N"))
$log = Join-Path $env:TEMP ("tessara-frames-" + [guid]::NewGuid().ToString("N") + ".txt")
& (Join-Path $bin "tsr-eval.exe") seed --dest $data | Out-Host
if ($LASTEXITCODE -ne 0) { Write-Error "seed failed"; exit 2 }

$fgBefore = [TsrMeasure.Win]::GetForegroundWindow()
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = Join-Path $bin "tessara.exe"
$psi.UseShellExecute = $false
$psi.EnvironmentVariables["TSR_DATA_DIR"] = $data
$psi.EnvironmentVariables["TSR_FRAME_LOG"] = $log
$app = [System.Diagnostics.Process]::Start($psi)
$code = 2
try {
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and $app.MainWindowHandle -eq 0 -and -not $app.HasExited) {
        Start-Sleep -Milliseconds 250; $app.Refresh()
    }
    Start-Sleep -Seconds 3
    if ($app.HasExited) { Write-Error "tessara.exe exited early"; exit 2 }
    $noStealAtStart = ([TsrMeasure.Win]::GetForegroundWindow() -eq $fgBefore)
    # Which panel was showing during that check (logical width of the window region): a peek
    # (380) appears at start when a default hotkey is taken, so the check covers a peek.
    $rb = New-Object TsrMeasure.Win+RECT
    [TsrMeasure.Win]::GetWindowRgnBox($app.MainWindowHandle, [ref]$rb) | Out-Null
    # A DPI-unaware host sees logical pixels, an aware one physical: accept either.
    $rw = $rb.Right - $rb.Left
    $scale = [TsrMeasure.Win]::GetDpiForWindow($app.MainWindowHandle) / 96.0
    $panelAtStart = "region width $rw"
    foreach ($c in @(@{ n = "pill"; w = 220 }, @{ n = "peek"; w = 380 })) {
        if ([math]::Abs($rw - $c.w) -le 2 -or [math]::Abs($rw - $c.w * $scale) -le 2) { $panelAtStart = $c.n }
    }

    $opened = 0; $focusReturned = 0; $perCycle = @()
    for ($i = 0; $i -lt $Cycles; $i++) {
        Press-Toggle; Start-Sleep -Milliseconds 1200
        $fgOpen = Foreground-Pid
        if ($fgOpen -eq $app.Id) { $opened++ }
        Press-Toggle; Start-Sleep -Milliseconds 1000
        $fgClose = [TsrMeasure.Win]::GetForegroundWindow()
        if ($fgClose -eq $fgBefore) { $focusReturned++ }
        # Per cycle: did the panel have focus after opening; was focus back after closing.
        $perCycle += ,@(($fgOpen -eq $app.Id), ($fgClose -eq $fgBefore))
    }
    Start-Sleep -Milliseconds 500

    $frames = @{ expand = @(); collapse = @(); frame_after_input_ms = @(); region_after_input_ms = @(); event_after_input_ms = @() }
    if (Test-Path $log) {
        foreach ($line in Get-Content $log) {
            $parts = $line -split ' '
            if ($frames.ContainsKey($parts[0])) { $frames[$parts[0]] += ($parts[1..($parts.Count - 1)] | ForEach-Object { [double]$_ }) }
        }
    }
    # A frame is "late" if it took longer than one 60 Hz interval plus 1 ms of timer jitter.
    $anim = @($frames.expand) + @($frames.collapse)
    $late = @($anim | Where-Object { $_ -gt 17.7 }).Count
    $lateShare = if ($anim.Count) { [math]::Round($late / $anim.Count, 4) } else { $null }
    # Input-to-visual per open = the later of "first animation frame painted" (UI) and "window
    # region applied" (Rust); the two are logged in the same order, one pair per open.
    $itv = @()
    $nPairs = [math]::Min(@($frames.frame_after_input_ms).Count, @($frames.region_after_input_ms).Count)
    for ($k = 0; $k -lt $nPairs; $k++) { $itv += [math]::Max($frames.frame_after_input_ms[$k], $frames.region_after_input_ms[$k]) }
    $lat = Stats $itv
    $report = [ordered]@{
        measured_at = (Get-Date).ToString("s")
        os = [Environment]::OSVersion.VersionString
        build = "debug Rust, production UI bundle (ui/dist)"
        cycles = $Cycles
        toggle_hotkey = $(if ($toggleVk -eq 0x20) { "Ctrl+Alt+Space" } else { "Ctrl+Alt+N (fallback: Ctrl+Alt+Space owned by another app)" })
        panel_focused_after_open = $opened
        per_cycle_focused_then_returned = $perCycle
        no_focus_steal_at_start = $noStealAtStart
        panel_during_start_check = $panelAtStart
        focus_returned_after_close = $focusReturned
        animation_frame_ms = Stats $anim
        expand_frame_ms = Stats $frames.expand
        collapse_frame_ms = Stats $frames.collapse
        late_frames = $late
        late_frame_share = $lateShare
        input_to_visual_ms = $lat
        input_to_visual_samples_ms = $itv
        frame_after_input_ms = Stats $frames.frame_after_input_ms
        region_after_input_ms = Stats $frames.region_after_input_ms
        event_after_input_ms = Stats $frames.event_after_input_ms
        thresholds = [ordered]@{ late_frame_share_max = 0.05; input_to_visual_p95_max_ms = 100 }
    }
    $pass = $noStealAtStart -and $opened -eq $Cycles -and $focusReturned -eq $Cycles -and
        $lateShare -ne $null -and $lateShare -le 0.05 -and $lat -and $lat.p95 -le 100
    $report.pass = [bool]$pass
    $out = Join-Path $root "evals\results\notch_perf.json"
    $report | ConvertTo-Json -Depth 5 | Out-File -Encoding utf8 $out
    Get-Content $out | Out-Host
    $code = if ($pass) { 0 } else { 1 }
}
finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    $app.WaitForExit(5000) | Out-Null
    Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
    Remove-Item -Force $log -ErrorAction SilentlyContinue
}
exit $code
