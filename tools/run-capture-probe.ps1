<#
.SYNOPSIS
  Stage 0 app-compatibility run: open synthetic fixtures in installed target apps and measure
  rung 1 (UIA) and rung 2 (WGC + Windows OCR) with bench/uia-probe (metrics only).

.DESCRIPTION
  Safety rules, so a run never reads the user's own content:
  - Browsers and VS Code run as separate instances with throwaway profiles in %TEMP%\tsr-probe,
    and only those instances' PIDs are probed.
  - Office apps and Notepad are skipped if they are already running (our file would open inside
    the user's session); otherwise they are started and closed again by PID.
  - Explorer windows are matched by the fixture folder name.
  - Apps that would show a signed-in account (Outlook, Teams, WhatsApp, OneNote) are not opened.
  Output: docs/stage0/app-compat-probe.csv (exe/class names and numbers only).
#>
param([int] $LoadWaitSec = 12)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$probe = Join-Path $root 'target\debug\uia-probe.exe'
$fx = Join-Path $root 'tests\fixtures\screens'
$csv = Join-Path $root 'docs\stage0\app-compat-probe.csv'
$scratch = Join-Path $env:TEMP 'tsr-probe'
if (-not (Test-Path $probe)) { throw "Build the probe first: cargo build -p uia-probe" }
if (Test-Path $csv) { Remove-Item $csv }
New-Item -ItemType Directory -Force $scratch | Out-Null
$started = New-Object System.Collections.Generic.List[int]
$results = [ordered]@{}

function Find-Exe([string[]] $candidates) {
    foreach ($c in $candidates) { if ($c -and (Test-Path $c)) { return $c } }
    return $null
}
function Pids-ByCmdline([string] $needle) {
    @(Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($needle) } | ForEach-Object { [int]$_.ProcessId })
}
function Run-Probe([string] $label, [string] $exe, [int[]] $pids, [string] $title, [string[]] $extra) {
    $a = @('--label', $label, '--exe', $exe, '--csv', $csv)
    foreach ($p in $pids) { $a += @('--pid', "$p") }
    if ($title) { $a += @('--title-contains', $title) }
    if ($extra) { $a += $extra }
    $out = & $probe @a 2>&1 | Out-String
    $rows = @($out -split "`n" | Where-Object { $_ -like "$label,*" })
    $results[$label] = if ($LASTEXITCODE -ne 0) { "PROBE FAILED: $($out.Trim() -split "`n" | Select-Object -Last 1)" } elseif ($rows.Count -eq 0) { 'no window found' } else { "$($rows.Count) window(s)" }
    Write-Host "$label -> $($results[$label])"
}

$edge = Find-Exe @("${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe")
$chrome = Find-Exe @("$env:ProgramFiles\Google\Chrome\Application\chrome.exe", "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe")
$code = Find-Exe @("$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe")
$office = "$env:ProgramFiles\Microsoft Office\root\Office16"

try {
    # --- Browsers: isolated instances, one window per fixture page -------------------------
    foreach ($b in @(@{ name = 'edge'; exe = $edge; img = 'msedge.exe' }, @{ name = 'chrome'; exe = $chrome; img = 'chrome.exe' })) {
        if (-not $b.exe) { $results["$($b.name)"] = 'not installed'; continue }
        $profile = Join-Path $scratch "$($b.name)-profile"
        foreach ($page in 'email', 'chat', 'jira', 'hinglish', 'password') {
            $url = ([Uri](Join-Path $fx "$page.html")).AbsoluteUri
            Start-Process $b.exe -ArgumentList "--user-data-dir=`"$profile`"", '--no-first-run', '--no-default-browser-check', '--disable-sync', '--new-window', $url
            Start-Sleep -Seconds 4
        }
        Start-Sleep -Seconds 3
        $pids = Pids-ByCmdline $profile
        foreach ($p in $pids) { $started.Add($p) }
        foreach ($page in 'email', 'chat', 'jira', 'hinglish') {
            Run-Probe "$($b.name)-$page" $b.img $pids "TSR fixture $page" @()
        }
        Run-Probe "$($b.name)-password" $b.img $pids 'TSR fixture password' @('--assert-absent', 'TSRCANARY-pw-7f3a91c2')
    }

    # --- VS Code: isolated user-data and extensions dirs -----------------------------------
    if ($code) {
        $ud = Join-Path $scratch 'vscode-user'
        Start-Process $code -ArgumentList '--user-data-dir', "`"$ud`"", '--extensions-dir', "`"$(Join-Path $scratch 'vscode-ext')`"", '--new-window', '--disable-workspace-trust', "`"$(Join-Path $fx 'notes.txt')`""
        Start-Sleep -Seconds $LoadWaitSec
        $pids = Pids-ByCmdline $ud
        foreach ($p in $pids) { $started.Add($p) }
        Run-Probe 'vscode' 'code.exe' $pids 'notes.txt' @()
    } else { $results['vscode'] = 'not installed' }

    # --- Notepad and Office: only if not already running ------------------------------------
    $single = @(
        @{ label = 'notepad'; img = 'notepad.exe'; exe = 'notepad.exe'; file = 'notes.txt' },
        @{ label = 'word'; img = 'winword.exe'; exe = "$office\WINWORD.EXE"; file = 'notes.txt' },
        @{ label = 'excel'; img = 'excel.exe'; exe = "$office\EXCEL.EXE"; file = 'tracker.csv' }
    )
    foreach ($s in $single) {
        $procName = [IO.Path]::GetFileNameWithoutExtension($s.img)
        if (Get-Process -Name $procName -ErrorAction SilentlyContinue) { $results[$s.label] = 'skipped: already running (would mix with your session)'; continue }
        if ($s.exe -ne 'notepad.exe' -and -not (Test-Path $s.exe)) { $results[$s.label] = 'not installed'; continue }
        $p = Start-Process $s.exe -ArgumentList "`"$(Join-Path $fx $s.file)`"" -PassThru
        Start-Sleep -Seconds $LoadWaitSec
        $pids = @(Get-Process -Name $procName -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
        foreach ($x in $pids) { $started.Add($x) }
        Run-Probe $s.label $s.img $pids ([IO.Path]::GetFileNameWithoutExtension($s.file)) @()
    }

    # --- PowerPoint: build a synthetic deck through COM (it does not open .rtf outlines from
    #     the command line). The .pptx is saved in %TEMP% because Office stamps the signed-in
    #     user's name into its metadata; it must never be written into the repo.
    if (Get-Process POWERPNT -ErrorAction SilentlyContinue) { $results['powerpoint'] = 'skipped: already running (would mix with your session)' }
    elseif (-not (Test-Path "$office\POWERPNT.EXE")) { $results['powerpoint'] = 'not installed' }
    else {
        $pp = New-Object -ComObject PowerPoint.Application
        try {
            $pres = $pp.Presentations.Add($true)
            $slide = $pres.Slides.Add(1, 2)
            $slide.Shapes.Item(1).TextFrame.TextRange.Text = 'H1 2027 roadmap'
            $slide.Shapes.Item(2).TextFrame.TextRange.Text = "Streaming exports (GA in 3.14)`rOkta SSO - planned Q1, not committed`rWebhook retry controls"
            $pres.SaveAs((Join-Path $scratch 'tsr-slides.pptx'))
            Start-Sleep -Seconds 3
            $pids = @(Get-Process POWERPNT | ForEach-Object { $_.Id })
            Run-Probe 'powerpoint' 'powerpnt.exe' $pids 'tsr-slides' @()
        } finally {
            try { $pres.Close() } catch {}
            $pp.Quit(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($pp)
            Start-Sleep -Seconds 2
            Get-Process POWERPNT -ErrorAction SilentlyContinue | Stop-Process -Force
        }
    }

    # --- File Explorer: shared process, match by folder title -------------------------------
    Start-Process explorer.exe -ArgumentList "`"$(Join-Path $fx 'tsr-fixture-folder')`""
    Start-Sleep -Seconds 4
    Run-Probe 'explorer' 'explorer.exe' @() 'tsr-fixture-folder' @()
}
finally {
    foreach ($p in ($started | Sort-Object -Unique)) { Stop-Process -Id $p -Force -ErrorAction SilentlyContinue }
    try {
        $shell = New-Object -ComObject Shell.Application
        foreach ($w in @($shell.Windows())) { if ($w.LocationURL -like '*tsr-fixture-folder*') { $w.Quit() } }
    } catch {}
    Start-Sleep -Seconds 2
    Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue
}

Write-Host "`nSummary"
$results.GetEnumerator() | ForEach-Object { "{0,-18} {1}" -f $_.Key, $_.Value } | Write-Host
Write-Host "Wrote $csv"
