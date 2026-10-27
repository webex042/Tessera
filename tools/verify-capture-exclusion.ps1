# Verifies with real pixels that screen capture cannot see the notch (plan §13.1, Stage 3).
#
#   powershell -ExecutionPolicy Bypass -File tools\verify-capture-exclusion.ps1 [-NoBuild] [-Panel pill|expanded]
#
# 1. Seeds a throwaway encrypted store with synthetic data (tsr-eval seed) in %TEMP%.
# 2. Starts tessara.exe on that store (TSR_DATA_DIR), so the real profile is never touched.
# 3. Runs tsr-verify-exclusion against that process: WGC monitor capture + GDI BitBlt, with
#    a magenta backdrop under the notch and two control windows (see the binary's doc comment).
# 4. Stops only the tessara.exe process this script started, and removes the temp store.
# Writes evals/results/capture_exclusion_<panel>.json. Exit code 0 = PASS, 1 = FAIL, 2 = could not run.
#
# The notch appears on screen for a few seconds, top-centre of the monitor under the cursor.
# Needs no elevation and no network.

param(
    [switch]$NoBuild,
    [ValidateSet("pill", "expanded")][string]$Panel = "pill"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target\debug"

if (-not $NoBuild) {
    Push-Location (Join-Path $root "ui"); pnpm build | Out-Null; $uiExit = $LASTEXITCODE; Pop-Location
    if ($uiExit -ne 0) { Write-Error "UI build failed"; exit 2 }
    cargo build -p tessara -p tsr-capture -p tsr-evals --features tessara/custom-protocol --bin tessara --bin tsr-verify-exclusion --bin tsr-eval
    if ($LASTEXITCODE -ne 0) { Write-Error "cargo build failed"; exit 2 }
}

$data = Join-Path $env:TEMP ("tessara-verify-" + [guid]::NewGuid().ToString("N"))
& (Join-Path $bin "tsr-eval.exe") seed --dest $data | Out-Host
if ($LASTEXITCODE -ne 0) { Write-Error "seed failed"; exit 2 }

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = Join-Path $bin "tessara.exe"
$psi.UseShellExecute = $false
$psi.EnvironmentVariables["TSR_DATA_DIR"] = $data
$psi.EnvironmentVariables["TSR_START_PANEL"] = $Panel
$app = [System.Diagnostics.Process]::Start($psi)
$code = 2
try {
    # Wait for the notch window (up to 20 s).
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and $app.MainWindowHandle -eq 0 -and -not $app.HasExited) {
        Start-Sleep -Milliseconds 250; $app.Refresh()
    }
    if ($app.HasExited) { Write-Error "tessara.exe exited early ($($app.ExitCode))"; exit 2 }
    # Wait until the window's region (the visible panel) has the requested panel's width
    # (expanded 760 logical px, pill 220), so the check measures the state it claims to.
    Add-Type -Namespace TsrVerify -Name Win -MemberDefinition @'
[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
public struct RECT { public int Left, Top, Right, Bottom; }
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool GetWindowRect(System.IntPtr h, out RECT r);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetDpiForWindow(System.IntPtr h);
[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern int GetWindowRgnBox(System.IntPtr h, out RECT r);
'@
    $wantLogical = if ($Panel -eq "expanded") { 760 } else { 220 }
    $deadline = (Get-Date).AddSeconds(20)
    $ready = $false
    while ((Get-Date) -lt $deadline -and -not $ready) {
        $r = New-Object TsrVerify.Win+RECT
        [TsrVerify.Win]::GetWindowRgnBox($app.MainWindowHandle, [ref]$r) | Out-Null
        $scale = [TsrVerify.Win]::GetDpiForWindow($app.MainWindowHandle) / 96.0
        # A DPI-unaware host sees logical pixels, an aware one physical: accept either.
        $w = $r.Right - $r.Left
        $ready = ([math]::Abs($w - $wantLogical * $scale) -le 2) -or ([math]::Abs($w - $wantLogical) -le 2)
        if (-not $ready) { Start-Sleep -Milliseconds 250 }
    }
    if (-not $ready) { Write-Error "the $Panel panel did not open within 20 s"; exit 2 }
    Start-Sleep -Milliseconds 800   # let the open animation finish and the WebView paint
    $json = & (Join-Path $bin "tsr-verify-exclusion.exe") --pid $app.Id
    $code = $LASTEXITCODE
    $out = Join-Path $root "evals\results\capture_exclusion_$Panel.json"
    $report = $json | ConvertFrom-Json
    $report | Add-Member -NotePropertyName panel -NotePropertyValue $Panel
    $report | Add-Member -NotePropertyName measured_at -NotePropertyValue ((Get-Date).ToString("s"))
    $report | Add-Member -NotePropertyName os -NotePropertyValue ([Environment]::OSVersion.VersionString)
    $report.PSObject.Properties.Remove("pid")
    $report | ConvertTo-Json -Depth 8 | Out-File -Encoding utf8 $out
    Get-Content $out | Out-Host
    Write-Host ("capture exclusion ({0}): {1} -> {2}" -f $Panel, ($(if ($code -eq 0) { "PASS" } else { "FAIL" })), $out)
}
finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    $app.WaitForExit(5000) | Out-Null
    Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
}
exit $code
