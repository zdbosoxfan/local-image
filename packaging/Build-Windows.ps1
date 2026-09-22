param(
    [string]$Python = 'python',
    [Parameter(Mandatory=$true)][string]$WebView2Package,
    [Parameter(Mandatory=$true)][string]$InnoCompiler,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\dist')
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = [IO.Path]::GetFullPath($OutputDirectory)
$package = Join-Path $output 'package'
$installer = Join-Path $output 'installer'
$prerequisites = Join-Path $output 'prerequisites'
New-Item -ItemType Directory -Path $package,$installer,$prerequisites -Force | Out-Null
$webViewSetup = Join-Path $prerequisites 'MicrosoftEdgeWebView2Setup.exe'
if (-not (Test-Path -LiteralPath $webViewSetup)) {
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $webViewSetup -UseBasicParsing
}
$signature = Get-AuthenticodeSignature -LiteralPath $webViewSetup
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
    throw 'The WebView2 bootstrapper does not have a valid Microsoft signature.'
}
& $Python -m PyInstaller --noconfirm --distpath $package --workpath (Join-Path $output 'build') (Join-Path $PSScriptRoot 'local-remove.spec')
if ($LASTEXITCODE -ne 0) { throw 'Backend packaging failed.' }
& (Join-Path $repository 'desktop\Build-NativeHost.ps1') -PackageRoot $WebView2Package -OutputDirectory $package
if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
Copy-Item -LiteralPath (Join-Path $repository 'README.md') -Destination $package -Force
& $InnoCompiler ('/DPackageDir=' + $package) ('/DInstallerDir=' + $installer) ('/DPrerequisiteDir=' + $prerequisites) (Join-Path $PSScriptRoot 'LocalRemove.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
Get-ChildItem -LiteralPath $installer -Filter '*.exe' | ForEach-Object {
    $checksum = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    ($checksum + '  ' + $_.Name) | Set-Content -LiteralPath ($_.FullName + '.sha256') -Encoding ASCII
    Write-Output $_.FullName
}
