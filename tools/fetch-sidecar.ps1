<#
.SYNOPSIS
  Thin wrapper around tools/fetch_sidecar.py (download, verify and unpack pinned binaries).

.DESCRIPTION
  The logic moved to Python because PowerShell 5.1 cannot rewrite the JSON lock file without
  reflowing it, and the lock now records a hash for every file in each build (see the .py header).

.EXAMPLE
  ./tools/fetch-sidecar.ps1 -Component llama_cpp -Variant vulkan-x64
  ./tools/fetch-sidecar.ps1 -Component onnxruntime -Variant win-x64
  ./tools/fetch-sidecar.ps1 -List
#>
param(
    [ValidateSet('llama_cpp', 'onnxruntime')] [string] $Component,
    [string] $Variant,
    [switch] $Record,
    [switch] $List
)
$ErrorActionPreference = 'Stop'
$py = Join-Path $PSScriptRoot 'fetch_sidecar.py'
$argsList = @()
if ($List) { $argsList += '--list' } else { $argsList += @($Component, $Variant) }
if ($Record) { $argsList += '--record' }
& python $py @argsList
exit $LASTEXITCODE
