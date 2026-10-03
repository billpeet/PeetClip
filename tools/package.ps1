param(
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [string]$Vpk = 'vpk',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $root
try {
    if (-not $SkipBuild) {
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    }
    $stage = [IO.Path]::GetFullPath((Join-Path $root 'target\package'))
    $output = Join-Path $root "target\releases\$Version"
    # Delete only the dedicated staging directory inside this workspace.
    if ($stage -ne [IO.Path]::Combine($root, 'target', 'package')) { throw 'Invalid staging path' }
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $build = Join-Path $root 'target\release'
    Copy-Item -LiteralPath (Join-Path $build 'peetclip.exe') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $stage
    Get-ChildItem -LiteralPath $build -File | Where-Object { $_.Extension -in '.dll', '.pri' } |
        Copy-Item -Destination $stage
    Get-ChildItem -LiteralPath $build -Directory | Where-Object {
        $_.Name -match '^[a-z]{2,3}(-[a-z0-9]{2,8})+$' -or $_.Name -eq 'Microsoft.UI.Xaml'
    } | Copy-Item -Destination $stage -Recurse
    foreach ($required in @('Microsoft.ui.xaml.dll', 'Microsoft.WindowsAppRuntime.dll', 'resources.pri')) {
        if (-not (Test-Path -LiteralPath (Join-Path $stage $required))) { throw "Missing runtime file: $required" }
    }
    & $Vpk pack --packId PeetClip --packTitle PeetClip --packAuthors billpeet `
        --packVersion $Version --packDir $stage --mainExe peetclip.exe `
        --icon (Join-Path $root 'assets\clipboard.ico') --outputDir $output `
        --shortcuts 'StartMenuRoot,Startup' --framework vcredist143-x64 --runtime win-x64 --delta None
    if ($LASTEXITCODE -ne 0) { throw 'Installer packaging failed' }
    $hashes = Get-ChildItem -LiteralPath $output -File | Where-Object Name -ne 'SHA256SUMS.txt' |
        Sort-Object Name | ForEach-Object { "{0}  {1}" -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name }
    $hashes | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
    Write-Output "Installer and update feed: $output"
} finally { Pop-Location }
