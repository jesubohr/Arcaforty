[CmdletBinding()]
param(
    [string]$CompilerPath,
    [string]$RuntimeDll,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    }
    if (-not $CompilerPath) {
        $compilerCandidates = @(
            "$env:LOCALAPPDATA/Programs/Inno Setup 6/ISCC.exe",
            "${env:ProgramFiles(x86)}/Inno Setup 6/ISCC.exe",
            "$env:ProgramFiles/Inno Setup 6/ISCC.exe"
        )
        $CompilerPath = $compilerCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    if (-not $CompilerPath) { throw 'Install Inno Setup 6, or pass -CompilerPath to ISCC.exe.' }
    if (-not $RuntimeDll) {
        $vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
        if (Test-Path -LiteralPath $vswhere) {
            $vsRoot = & $vswhere -latest -products '*' -property installationPath
            $runtimeCandidates = Get-ChildItem -Path "$vsRoot/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT/vcruntime140.dll" -ErrorAction SilentlyContinue
            $RuntimeDll = $runtimeCandidates | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
        }
    }
    if (-not $RuntimeDll -or -not (Test-Path -LiteralPath $RuntimeDll)) {
        throw 'Pass -RuntimeDll with the x64 vcruntime140.dll from the Visual Studio redistributable folder.'
    }
    $metadataText = & cargo metadata --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read dependency metadata.' }
    $metadata = $metadataText | ConvertFrom-Json
    $app = $metadata.packages | Where-Object { $_.name -eq 'arcaforty' } | Select-Object -First 1
    $version = $app.version
    $staging = Join-Path $projectRoot 'dist/staging'
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    $notices = [System.Text.StringBuilder]::new()
    [void]$notices.AppendLine('Arcaforty third-party dependency notices')
    [void]$notices.AppendLine('Includes dependencies resolved in Cargo.lock, including platform-specific packages.')
    foreach ($package in ($metadata.packages | Where-Object { $_.source } | Sort-Object name,version)) {
        [void]$notices.AppendLine("`n===== $($package.name) $($package.version) =====")
        [void]$notices.AppendLine("License expression: $($package.license)")
        [void]$notices.AppendLine("Repository: $($package.repository)")
        $packageDir = Split-Path -Parent $package.manifest_path
        $licenseFiles = @(Get-ChildItem -LiteralPath $packageDir -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)' })
        foreach ($folder in @('license','licenses')) {
            $licenseDir = Join-Path $packageDir $folder
            if (Test-Path -LiteralPath $licenseDir) { $licenseFiles += Get-ChildItem -LiteralPath $licenseDir -File -Recurse }
        }
        if ($package.license_file) {
            $licensePath = Join-Path $packageDir $package.license_file
            if (Test-Path -LiteralPath $licensePath) { $licenseFiles += Get-Item -LiteralPath $licensePath }
        }
        foreach ($file in ($licenseFiles | Sort-Object FullName -Unique)) {
            [void]$notices.AppendLine("--- $($file.Name) ---")
            [void]$notices.AppendLine([System.IO.File]::ReadAllText($file.FullName))
        }
    }
    [void]$notices.AppendLine("`nMicrosoft Visual C++ runtime: vcruntime140.dll, distributed app-local from the Visual Studio redistributable directory. Microsoft owns this component. See https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files")
    [System.IO.File]::WriteAllText((Join-Path $staging 'THIRD-PARTY-NOTICES.txt'), $notices.ToString())
    & $CompilerPath "/DAppVersion=$version" "/DRuntimeDll=$RuntimeDll" (Join-Path $PSScriptRoot 'arcaforty.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
    $installer = Join-Path $projectRoot "dist/Arcaforty-Setup-$version-x64.exe"
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $(Split-Path -Leaf $installer)" | Set-Content -LiteralPath "$installer.sha256" -Encoding ascii
    Write-Output $installer
} finally {
    Pop-Location
}
