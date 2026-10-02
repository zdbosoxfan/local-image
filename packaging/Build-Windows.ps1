param(
    [string]$Python = 'python',
    [Parameter(Mandatory=$true)][string]$WebView2Package,
    [Parameter(Mandatory=$true)][string]$InnoCompiler,
    [string]$OutputDirectory = ''
)
$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot '..\dist' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
foreach ($frontendFile in @('backend\frontend_dist\.vite\manifest.json', 'backend\frontend_dist\THIRD_PARTY_NOTICES.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $repository $frontendFile) -PathType Leaf)) {
        throw 'Build frontend/ and its dependency license notices before Windows packaging. End users do not need Node or Vite.'
    }
}
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
& $Python -m PyInstaller --noconfirm --distpath $package --workpath (Join-Path $output 'build') (Join-Path $PSScriptRoot 'local-image.spec')
if ($LASTEXITCODE -ne 0) { throw 'Backend packaging failed.' }
& (Join-Path $repository 'desktop\Build-NativeHost.ps1') -PackageRoot $WebView2Package -OutputDirectory $package
if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
Copy-Item -LiteralPath (Join-Path $repository 'README.md') -Destination $package -Force
Copy-Item -LiteralPath (Join-Path $repository 'docs') -Destination $package -Recurse -Force
$packageEntries = Get-ChildItem -LiteralPath $package -Recurse -Force
$longestFile = ($packageEntries | Where-Object { -not $_.PSIsContainer } | ForEach-Object { $_.FullName.Length - $package.Length - 1 } | Measure-Object -Maximum).Maximum
$longestDirectory = ($packageEntries | Where-Object { $_.PSIsContainer } | ForEach-Object { $_.FullName.Length - $package.Length - 1 } | Measure-Object -Maximum).Maximum
# Keep two characters of margin below Win32's file and directory path limits.
$appPathLimit = [Math]::Min(256 - $longestFile, 244 - $longestDirectory)
if ($appPathLimit -lt 64) { throw 'The package contains paths too deep for a normal Windows installation.' }
& $InnoCompiler ('/DPackageDir=' + $package) ('/DInstallerDir=' + $installer) ('/DPrerequisiteDir=' + $prerequisites) ('/DAppPathLimit=' + $appPathLimit) (Join-Path $PSScriptRoot 'LocalImage.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
Get-ChildItem -LiteralPath $installer -Filter '*.exe' | ForEach-Object {
    $checksum = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    ($checksum + '  ' + $_.Name) | Set-Content -LiteralPath ($_.FullName + '.sha256') -Encoding ASCII
    Write-Output $_.FullName
}
