$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Installer smoke tests run only on disposable GitHub runners.' }
$build = [int][regex]::Match((Get-Content -Raw src/version.rs), 'BUILD: u32 = (\d+)').Groups[1].Value
$version = 'v{0:D3}' -f $build
$installer = (Resolve-Path "target/package/CellTray-$version-Setup-x64.exe").Path
$tempRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP)
$installDir = [IO.Path]::GetFullPath((Join-Path $tempRoot 'CellTray-installer-smoke'))
if (-not $installDir.StartsWith($tempRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe smoke-test destination' }
$settings = Join-Path $env:LOCALAPPDATA 'CellTray\settings.json'
if (Test-Path -LiteralPath $settings) { throw 'Refusing to overwrite existing runner preferences' }
$setupArguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/DIR=`"$installDir`"", '/TASKS=""', "/LOG=`"$env:RUNNER_TEMP\celltray-install.log`"")
function Install-CellTray {
    $process = Start-Process -FilePath $installer -ArgumentList $setupArguments -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
}
Install-CellTray
foreach ($file in @('celltray-widgets.exe', 'celltray.exe', 'celltray-updater.exe', 'unins000.exe', 'LICENSE', 'DEPENDENCIES.md')) {
    if (-not (Test-Path -LiteralPath (Join-Path $installDir $file))) { throw "Missing installed file: $file" }
}
$cliVersion = & (Join-Path $installDir 'celltray.exe') --version
if ($cliVersion -ne "CellTray $version") { throw "Wrong installed version: $cliVersion" }
$fileVersion = (Get-Item -LiteralPath (Join-Path $installDir 'celltray-widgets.exe')).VersionInfo.ProductVersion
if ($fileVersion -ne $version) { throw "Wrong file metadata version: $fileVersion" }
$copyright = 'Copyright (c) 2026 Baz Studios LLC'
if ((Get-Item -LiteralPath (Join-Path $installDir 'celltray-widgets.exe')).VersionInfo.LegalCopyright -ne $copyright) { throw 'Wrong embedded copyright holder' }
if ((Get-Content -LiteralPath (Join-Path $installDir 'LICENSE')) -notcontains $copyright) { throw 'Wrong installed license copyright holder' }
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'CellTray\CellTray.lnk'
if (-not (Test-Path -LiteralPath $shortcut)) { throw 'Start menu shortcut is missing' }
$startupKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Get-ItemProperty -LiteralPath $startupKey -Name CellTray -ErrorAction SilentlyContinue) { throw 'Startup should be opt-in' }
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{D168A6D4-1A3C-4775-A736-3F88A12DCD14}_is1'
if ((Get-ItemProperty -LiteralPath $registry).DisplayVersion -ne $version) { throw 'Installed Apps entry is missing or wrong' }
New-Item -ItemType Directory -Force -Path (Split-Path $settings -Parent) | Out-Null
$sentinel = '{"refresh_seconds":15,"cell_order":[2,0,1],"yellow_percent":25,"red_percent":10}'
[IO.File]::WriteAllText($settings, $sentinel)
# Exercise the updater on this disposable installation without launching the GUI.
$archive = (Resolve-Path "target/package/CellTray-$version-windows-x64.zip").Path
$archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$oldProcess = Start-Process -FilePath (Join-Path $installDir 'celltray.exe') -ArgumentList '--version' -PassThru -Wait -WindowStyle Hidden
foreach ($scenario in @('tampered', 'cancelled', 'success')) {
    $updateStage = Join-Path $tempRoot "CellTray-update-$scenario"
    New-Item -ItemType Directory -Path $updateStage | Out-Null
    $updateArchive = Join-Path $updateStage 'package.zip'
    Copy-Item -LiteralPath $archive -Destination $updateArchive
    $helper = Join-Path $updateStage 'celltray-updater.exe'
    Copy-Item -LiteralPath (Join-Path $installDir 'celltray-updater.exe') -Destination $helper
    $plan = @{
        version = $build; archive = $updateArchive; sha256 = $archiveHash
        target = $installDir; parent_pid = $oldProcess.Id; relaunch = $false
    }
    if ($scenario -eq 'tampered') { $plan.sha256 = '0' * 64 }
    if ($scenario -ne 'cancelled') { [IO.File]::WriteAllText((Join-Path $updateStage 'commit'), 'commit') }
    $planPath = Join-Path $updateStage 'plan.json'
    [IO.File]::WriteAllText($planPath, ($plan | ConvertTo-Json))
    $process = Start-Process -FilePath $helper -ArgumentList @('--apply', "`"$planPath`"") -PassThru -Wait -WindowStyle Hidden
    if ($scenario -eq 'tampered') {
        if ($process.ExitCode -eq 0) { throw 'Updater accepted an invalid digest' }
        if (Test-Path -LiteralPath (Join-Path $updateStage 'backup')) { throw 'Invalid update changed the installation' }
    } elseif ($scenario -eq 'cancelled') {
        if ($process.ExitCode -ne 0) { throw 'Update cancellation failed' }
        if (Test-Path -LiteralPath (Join-Path $updateStage 'backup')) { throw 'Uncommitted update changed the installation' }
    } else {
        if ($process.ExitCode -ne 0) { throw 'Internal update failed' }
        $result = Get-Content -Raw -LiteralPath (Join-Path $updateStage 'result.json') | ConvertFrom-Json
        if (-not $result.success) { throw 'Internal update did not complete' }
        if (-not (Test-Path -LiteralPath (Join-Path $updateStage 'backup\celltray-widgets.exe'))) { throw 'Update backup missing' }
    }
    if ((& (Join-Path $installDir 'celltray.exe') --version) -ne "CellTray $version") { throw 'Update damaged the installed app' }
    if ((Get-Content -Raw -LiteralPath $settings) -ne $sentinel) { throw 'Update changed preferences' }
}
# A newly added notice must also be removed by the original uninstaller.
[IO.File]::WriteAllText((Join-Path $installDir 'licenses\update-added-notice.txt'), 'Test notice')
$setupArguments = $setupArguments | ForEach-Object { if ($_ -eq '/TASKS=""') { '/TASKS="startup,desktopicon"' } else { $_ } }
Install-CellTray
if ((Get-Content -Raw -LiteralPath $settings) -ne $sentinel) { throw 'Reinstallation changed preferences' }
$startupCommand = (Get-ItemProperty -LiteralPath $startupKey -Name CellTray).CellTray
if ($startupCommand -ne ('"' + (Join-Path $installDir 'celltray-widgets.exe') + '"')) { throw 'Startup registration is wrong' }
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'CellTray.lnk'
if (-not (Test-Path -LiteralPath $desktopShortcut)) { throw 'Optional desktop shortcut is missing' }
$uninstaller = Join-Path $installDir 'unins000.exe'
$process = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' -PassThru -Wait -WindowStyle Hidden
if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode)" }
if (Test-Path -LiteralPath (Join-Path $installDir 'celltray-widgets.exe')) { throw 'Uninstall left the application binary behind' }
if (Test-Path -LiteralPath (Join-Path $installDir 'licenses')) { throw 'Uninstall left updated dependency notices behind' }
if (Test-Path -LiteralPath $registry) { throw 'Uninstall left the Installed Apps entry behind' }
if (Test-Path -LiteralPath $shortcut) { throw 'Uninstall left the Start menu shortcut behind' }
if (Test-Path -LiteralPath $desktopShortcut) { throw 'Uninstall left the desktop shortcut behind' }
if (Get-ItemProperty -LiteralPath $startupKey -Name CellTray -ErrorAction SilentlyContinue) { throw 'Uninstall left startup registration behind' }
if ((Get-Content -Raw -LiteralPath $settings) -ne $sentinel) { throw 'Uninstall removed user preferences' }
Remove-Item -LiteralPath $settings
Write-Output "Verified $version install, internal update, tamper rejection, cancellation, repair, Start menu, Installed Apps, version resources, uninstall and settings preservation."
