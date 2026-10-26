<#
.SYNOPSIS
  Download pinned external binaries listed in tools/sidecar.lock, verify their SHA-256, and unpack them.

.DESCRIPTION
  Plan section 7.1 (llama-server sidecar) and section 10.1 (ONNX Runtime). Nothing here runs at app runtime;
  this is a developer/CI script. Archives are rejected if their hash differs from the lock.
  Extracted files listed under "files" are also hash-checked. A null entry fails the run unless
  -Record is given, in which case the observed hash is written back to the lock for review.

.EXAMPLE
  ./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64
  ./tools/fetch-sidecar.ps1 -Component llama_cpp -Variant cpu-x64 -Record
#>
param(
    [Parameter(Mandatory)] [ValidateSet('llama_cpp', 'onnxruntime')] [string] $Component,
    [Parameter(Mandatory)] [string] $Variant,
    [switch] $Record
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$root = Split-Path -Parent $PSScriptRoot
$lockPath = Join-Path $PSScriptRoot 'sidecar.lock'
# Read as UTF-8 explicitly: Windows PowerShell 5.1 would otherwise use the ANSI code page.
$lockText = [IO.File]::ReadAllText($lockPath, [Text.Encoding]::UTF8)
$lock = $lockText | ConvertFrom-Json

$asset = $lock.$Component.assets | Where-Object { $_.variant -eq $Variant }
if (-not $asset) { throw "No asset '$Variant' for '$Component' in sidecar.lock" }

$version = if ($Component -eq 'llama_cpp') { $lock.llama_cpp.tag } else { $lock.onnxruntime.version }
$dest = Join-Path $root "third_party\$Component\$version\$Variant"
$cache = Join-Path $root 'third_party\.cache'
New-Item -ItemType Directory -Force $dest, $cache | Out-Null

$archive = Join-Path $cache $asset.name
if (-not (Test-Path $archive) -or (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower() -ne $asset.sha256) {
    Write-Host "Downloading $($asset.url)"
    Invoke-WebRequest -UseBasicParsing $asset.url -OutFile $archive
}
$hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $asset.sha256) {
    Remove-Item $archive -Force
    throw "Archive hash mismatch for $($asset.name): expected $($asset.sha256), got $hash"
}
Write-Host "Archive OK  $($asset.name)  sha256=$hash"

Expand-Archive $archive -DestinationPath $dest -Force

$changed = $false
foreach ($prop in $asset.files.PSObject.Properties) {
    $file = Join-Path $dest $prop.Name
    if (-not (Test-Path $file)) { throw "Expected file missing after extraction: $($prop.Name)" }
    $fh = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
    if ($null -eq $prop.Value) {
        if (-not $Record) { throw "No recorded hash for $($prop.Name); rerun with -Record on a trusted machine" }
        # Patch the text in place (scoped to this asset's block) instead of re-serialising:
        # ConvertTo-Json in PowerShell 5.1 reflows the whole file and makes reviews noisy.
        $start = $lockText.IndexOf("""variant"": ""$Variant""")
        $needle = """$($prop.Name)"": null"
        $at = $lockText.IndexOf($needle, $start)
        if ($start -lt 0 -or $at -lt 0) { throw "Could not locate the null entry for $($prop.Name) in sidecar.lock" }
        $lockText = $lockText.Remove($at, $needle.Length).Insert($at, """$($prop.Name)"": ""$fh""")
        $changed = $true
        Write-Host "Recorded    $($prop.Name)  sha256=$fh"
    } elseif ($prop.Value -ne $fh) {
        throw "File hash mismatch for $($prop.Name): expected $($prop.Value), got $fh"
    } else {
        Write-Host "File OK     $($prop.Name)  sha256=$fh"
    }
}

if ($changed) {
    [IO.File]::WriteAllText($lockPath, $lockText, (New-Object Text.UTF8Encoding $false))
    Write-Host "Updated $lockPath - review and commit the new hashes."
}
Write-Host "Unpacked to $dest"
