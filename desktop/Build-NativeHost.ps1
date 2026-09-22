param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'native-host'),
    [string]$PackageRoot = (Join-Path $PSScriptRoot 'webview2-sdk\package')
)
$ErrorActionPreference = 'Stop'
$framework = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$compiler = Join-Path $framework 'csc.exe'
if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'lib\net462\Microsoft.Web.WebView2.Core.dll'))) {
    throw 'Extract the official Microsoft.Web.WebView2 1.0.4191.47 NuGet package into webview2-sdk\package first.'
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$assembly = Join-Path $OutputDirectory 'Local Remove.exe'
$references = @('System.dll','System.Core.dll','System.Drawing.dll','System.Windows.Forms.dll','System.Net.Http.dll','System.Web.Extensions.dll')
$arguments = @('/nologo','/target:winexe','/platform:x64','/optimize+','/utf8output',('/out:' + $assembly),('/win32manifest:' + (Join-Path $PSScriptRoot 'LocalRemove.manifest')),('/win32icon:' + (Join-Path $PSScriptRoot 'icon\local-remove.ico')))
$arguments += $references | ForEach-Object { '/reference:' + (Join-Path $framework $_) }
$arguments += '/reference:' + (Join-Path $packageRoot 'lib\net462\Microsoft.Web.WebView2.Core.dll')
$arguments += '/reference:' + (Join-Path $packageRoot 'lib\net462\Microsoft.Web.WebView2.WinForms.dll')
$arguments += Join-Path $PSScriptRoot 'LocalRemoveLauncher.cs'
& $compiler $arguments
if ($LASTEXITCODE -ne 0) { throw "Native host compilation failed ($LASTEXITCODE)." }
Copy-Item -LiteralPath (Join-Path $packageRoot 'lib\net462\Microsoft.Web.WebView2.Core.dll') -Destination $OutputDirectory -Force
Copy-Item -LiteralPath (Join-Path $packageRoot 'lib\net462\Microsoft.Web.WebView2.WinForms.dll') -Destination $OutputDirectory -Force
Copy-Item -LiteralPath (Join-Path $packageRoot 'runtimes\win-x64\native\WebView2Loader.dll') -Destination $OutputDirectory -Force
Copy-Item -LiteralPath (Join-Path $packageRoot 'LICENSE.txt') -Destination (Join-Path $OutputDirectory 'Microsoft-WebView2-LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $packageRoot 'NOTICE.txt') -Destination (Join-Path $OutputDirectory 'Microsoft-WebView2-NOTICE.txt') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'NativeHost-README.md') -Destination $OutputDirectory -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'icon\local-remove.ico') -Destination $OutputDirectory -Force
Get-ChildItem -LiteralPath $OutputDirectory -File | Select-Object Name,Length
