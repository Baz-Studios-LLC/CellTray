$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $repoRoot
python tools/package_windows.py
if ($LASTEXITCODE -ne 0) { throw 'Package staging failed' }
$buildMatch = [regex]::Match((Get-Content -Raw src/version.rs), 'BUILD: u32 = (\d+)')
$version = 'v{0:D3}' -f [int]$buildMatch.Groups[1].Value
$output = Join-Path $repoRoot 'target\package'
$stage = Join-Path $output "CellTray-$version-windows-x64"
$candidates = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe")
$compiler = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $compiler) { throw 'Install Inno Setup 6, or build with the Windows installer GitHub workflow.' }
& $compiler "/DProductVersion=$version" "/DStageDir=$stage" "/DOutputDir=$output" installer/CellTray.iss
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
$assets = @("CellTray-$version-Setup-x64.exe", "CellTray-$version-windows-x64.zip")
$lines = foreach ($asset in $assets) {
    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $output $asset)
    "$($hash.Hash.ToLowerInvariant())  $asset"
}
[IO.File]::WriteAllLines((Join-Path $output 'SHA256SUMS.txt'), $lines, [Text.Encoding]::ASCII)
if ($env:GITHUB_OUTPUT) { "version=$version" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT }
