<#
.SYNOPSIS
  Builds the Xbox (UWP) package of Super Smash Bros. 3DS Recompiled for one
  region: the USA and Japan releases are separate apps.

.DESCRIPTION
  Run from a "Developer PowerShell for VS" (or after ilammy/msvc-dev-cmd with
  uwp: true), so that link.exe uses the app ("store") CRT libraries. Needs the
  nightly Rust toolchain with rust-src, and the Windows 10/11 SDK for
  makeappx.exe and signtool.exe.

  Recompiled code from your own dump is linked in when SMASH3DS_RECOMP_DIR
  points at the `code` folder that `3dsrecomp port` wrote; otherwise the game
  runs in the interpreter. See docs/XBOX.md.

.EXAMPLE
  ./packaging/uwp/build-msix.ps1 -Region USA
  ./packaging/uwp/build-msix.ps1 -Region JPN -RecompDir C:\ports\smash-jpn\code
#>
param(
  [Parameter(Mandatory = $true)][ValidateSet('USA', 'JPN')][string]$Region,
  # four-part package version; defaults to the workspace version plus .0
  [string]$Version,
  # recompiled code for this region, overriding SMASH3DS_RECOMP_DIR
  [string]$RecompDir,
  # a .pfx to sign with; a self-signed one is made when none is given
  [string]$CertificatePath,
  [string]$CertificatePassword = 'smash3ds',
  [string]$OutDir = 'dist/uwp'
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '../..')
Set-Location $root

$publisher = 'CN=SmashBros3DSRecompiled'
$regions = @{
  USA = @{ Feature = 'usa'; Identity = 'SmashBros3DSRecompiled.USA'; Name = 'Super Smash Bros. 3DS Recompiled (USA)'; Language = 'en-us' }
  JPN = @{ Feature = 'jpn'; Identity = 'SmashBros3DSRecompiled.JPN'; Name = 'Super Smash Bros. 3DS Recompiled (Japan)'; Language = 'ja-jp' }
}
$info = $regions[$Region]

if (-not $Version) {
  $line = Select-String -Path Cargo.toml -Pattern '^version = "(\d+\.\d+\.\d+)"' | Select-Object -First 1
  $Version = "$($line.Matches[0].Groups[1].Value).0"
}

# --- build -------------------------------------------------------------------
if ($RecompDir) { $env:SMASH3DS_RECOMP_DIR = (Resolve-Path $RecompDir).Path }
$target = 'x86_64-uwp-windows-msvc'
Write-Host "Building the $Region app ($target, version $Version)"
cargo +nightly build -Z build-std --release --locked --target $target -p zakuro-uwp --no-default-features --features $info.Feature
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

# --- lay the package out -----------------------------------------------------
$layout = Join-Path $OutDir "layout-$Region"
Remove-Item -Recurse -Force $layout -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path (Join-Path $layout 'Assets') | Out-Null
Copy-Item "target/$target/release/smash-3ds-uwp.exe" $layout
Copy-Item packaging/uwp/Assets/*.png (Join-Path $layout 'Assets')
Copy-Item README.md, COMPATIBILITY.md, LICENSE, LICENSE-MIT, THIRD_PARTY_NOTICES.md $layout
Copy-Item docs/XBOX.md $layout

$linked = if ($env:SMASH3DS_RECOMP_DIR) { 'with recompiled code' } else { 'interpreter build' }
$manifest = (Get-Content packaging/uwp/AppxManifest.template.xml -Raw).
  Replace('{{IDENTITY}}', $info.Identity).
  Replace('{{PUBLISHER}}', $publisher).
  Replace('{{VERSION}}', $Version).
  Replace('{{DISPLAY_NAME}}', $info.Name).
  Replace('{{LANGUAGE}}', $info.Language).
  Replace('{{DESCRIPTION}}', "Super Smash Bros. for Nintendo 3DS ($Region release), $linked. Put your own decrypted dump in D:\DevelopmentFiles\SmashBros3DS-$Region.")
Set-Content -Path (Join-Path $layout 'AppxManifest.xml') -Value $manifest -Encoding UTF8

# --- pack and sign -----------------------------------------------------------
function Find-SdkTool($name) {
  $found = Get-Command $name -ErrorAction SilentlyContinue
  if ($found) { return $found.Source }
  $kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
  $tool = Get-ChildItem -Path $kits -Recurse -Filter $name -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $tool) { throw "$name not found; install the Windows 10/11 SDK" }
  return $tool.FullName
}
$makeappx = Find-SdkTool 'makeappx.exe'
$signtool = Find-SdkTool 'signtool.exe'

$package = Join-Path $OutDir "smash-3ds-xbox-$($Region.ToLower())-x64.msix"
& $makeappx pack /o /d $layout /p $package
if ($LASTEXITCODE -ne 0) { throw 'makeappx failed' }

if (-not $CertificatePath) {
  # Xbox Dev Mode installs packages signed by any certificate whose subject
  # matches the manifest's Publisher; a PC needs the .cer trusted first.
  $CertificatePath = Join-Path $OutDir 'smash-3ds-xbox.pfx'
  if (-not (Test-Path $CertificatePath)) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $publisher -KeyUsage DigitalSignature `
      -FriendlyName 'Super Smash Bros. 3DS Recompiled (Xbox)' -CertStoreLocation 'Cert:\CurrentUser\My' `
      -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
    $secure = ConvertTo-SecureString -String $CertificatePassword -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $CertificatePath -Password $secure | Out-Null
    Export-Certificate -Cert $cert -FilePath (Join-Path $OutDir 'smash-3ds-xbox.cer') | Out-Null
  }
}
& $signtool sign /fd SHA256 /a /f $CertificatePath /p $CertificatePassword $package
if ($LASTEXITCODE -ne 0) { throw 'signtool failed' }

# --- the VCLibs framework package the app depends on --------------------------
$vclibs = Get-ChildItem -Path "${env:ProgramFiles(x86)}\Microsoft SDKs\Windows Kits\10\ExtensionSDKs\Microsoft.VCLibs" -Recurse `
  -Filter 'Microsoft.VCLibs.x64.14.00.appx' -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match '\\Retail\\' } | Select-Object -First 1
if ($vclibs) {
  Copy-Item $vclibs.FullName (Join-Path $OutDir 'Microsoft.VCLibs.x64.14.00.appx') -Force
} else {
  Write-Warning 'Microsoft.VCLibs.x64.14.00.appx was not found in the Windows SDK; add it as a dependency when installing.'
}

Write-Host "Built $package ($linked)"
