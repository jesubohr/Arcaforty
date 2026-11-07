[CmdletBinding()]
param([string]$InstallerPath)

$ErrorActionPreference = 'Stop'
if (-not $InstallerPath) { $InstallerPath = Join-Path $PSScriptRoot '../dist/Arcaforty-Setup-0.1.0-x64.exe' }
$uninstallKey = 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/{75F2BB5C-C615-43BC-8FE8-2E123DF50B54}_is1'
if (Test-Path -LiteralPath $uninstallKey) {
    throw 'An Arcaforty installation is registered. Run this test on a clean account.'
}
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('Arcaforty-Installer-Test-' + [guid]::NewGuid())
$installDir = Join-Path $testRoot 'program'
New-Item -ItemType Directory -Path $testRoot | Out-Null
$arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/TASKS=', '/GROUP="Arcaforty Installer Test"', "/DIR=`"$installDir`"", "/LOG=`"$testRoot/install.log`"")
$process = Start-Process -FilePath (Resolve-Path -LiteralPath $InstallerPath).Path -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Install failed: $($process.ExitCode). Logs: $testRoot" }
foreach ($name in @('arcaforty.exe', 'vcruntime140.dll', 'README.md', 'THIRD-PARTY-NOTICES.txt', 'unins000.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $installDir $name))) { throw "Missing installed file: $name" }
}
$sourceExe = Join-Path $PSScriptRoot '../target/release/arcaforty.exe'
if ((Get-FileHash -LiteralPath $sourceExe).Hash -ne (Get-FileHash -LiteralPath (Join-Path $installDir 'arcaforty.exe')).Hash) { throw 'Installed executable differs.' }
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Arcaforty/Arcaforty.lnk'
if (-not (Test-Path -LiteralPath $shortcut)) { throw 'Start menu shortcut missing.' }
$sentinel = Join-Path $installDir 'user-data-sentinel.txt'
'Preserve user-created files.' | Set-Content -LiteralPath $sentinel
$process = Start-Process -FilePath (Join-Path $installDir 'unins000.exe') -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/LOG=`"$testRoot/uninstall.log`"") -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Uninstall failed: $($process.ExitCode)" }
if (Test-Path -LiteralPath (Join-Path $installDir 'arcaforty.exe')) { throw 'Uninstall left the app executable.' }
if (Test-Path -LiteralPath $shortcut) { throw 'Uninstall left the Start menu shortcut.' }
if (-not (Test-Path -LiteralPath $sentinel)) { throw 'Uninstall removed user-created files.' }
if (Test-Path -LiteralPath $uninstallKey) { throw 'Uninstall left its registry entry.' }
Write-Output "Install/uninstall passed. No app launched. Logs and sentinel retained at $testRoot"
