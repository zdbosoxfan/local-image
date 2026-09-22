$ErrorActionPreference='Stop'
$connector='C:\Users\Owner\Documents\RapidRAW-AI-Connector'
$desktop='C:\Users\Owner\Documents\Local Remove'
$shortcut='C:\Users\Owner\Desktop\Local Remove.lnk'
$queue=Invoke-RestMethod -Uri 'http://127.0.0.1:8189/queue' -TimeoutSec 10
if ($queue.queue_running.Count -or $queue.queue_pending.Count) {throw 'GPU is busy. Wait before installing.'}
$files=@('local_remove.py','local_remove.html','local_remove_project.py')
$nativeFiles=@('Local Remove.exe','NativeHost-README.md')
foreach($name in $files) {if(!(Test-Path -LiteralPath (Join-Path $PSScriptRoot $name))) {throw "Missing staged file: $name"}}
foreach($name in $nativeFiles) {if(!(Test-Path -LiteralPath (Join-Path $PSScriptRoot ('native-host\'+$name)))) {throw "Missing staged native file: $name"}}
$backup=Join-Path $connector ('backups\editable-projects-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path (Join-Path $backup 'desktop') -Force | Out-Null
foreach($name in $files) { $old=Join-Path $connector $name; if(Test-Path -LiteralPath $old) { $target=Join-Path $backup $name; New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null; Copy-Item -LiteralPath $old -Destination $target } }
foreach($name in $nativeFiles) {$old=Join-Path $desktop $name;if(Test-Path -LiteralPath $old) {Copy-Item -LiteralPath $old -Destination (Join-Path $backup ('desktop\'+$name))}}
if(Test-Path -LiteralPath $shortcut) {Copy-Item -LiteralPath $shortcut -Destination (Join-Path $backup 'desktop\Local Remove.lnk')}
# Update the launcher first, keeping an already running window alive.
foreach($name in $nativeFiles) {
    $destination=Join-Path $desktop $name
    $staged=Join-Path $PSScriptRoot ('native-host\'+$name)
    try {Copy-Item -LiteralPath $staged -Destination $destination -Force}
    catch {
        if($name -ne 'Local Remove.exe') {throw}
        $previous=Join-Path $desktop ('Local Remove.previous-'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'.exe')
        if((Split-Path ([IO.Path]::GetFullPath($previous))) -ne $desktop) {throw 'Invalid previous executable destination'}
        # Windows permits renaming the running executable without ending its process.
        Move-Item -LiteralPath $destination -Destination $previous -ErrorAction Stop
        try {Copy-Item -LiteralPath $staged -Destination $destination -ErrorAction Stop}
        catch {Move-Item -LiteralPath $previous -Destination $destination -ErrorAction Stop;throw}
    }
}
$processes=Get-CimInstance Win32_Process | Where-Object { $_.Name -in @('python.exe','pythonw.exe') -and $_.CommandLine -like '*Documents\RapidRAW-AI-Connector*main:app*' -and $_.CommandLine -match '--port\s+5000(?:\s|$)' }
foreach($process in $processes) {Stop-Process -Id $process.ProcessId -ErrorAction Stop}
foreach($name in $files) { $target=Join-Path $connector $name; New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null; Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $target -Force }
$startup=New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ShowWindow=[uint16]0}
$result=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{CommandLine='powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\Users\Owner\Documents\RapidRAW-AI-Connector\Start-RapidRAW-AI.ps1" -NoOpen';ProcessStartupInformation=$startup}
if($result.ReturnValue -ne 0) {throw 'Backend launcher did not start.'}
@{Backup=$backup;LauncherPid=$result.ProcessId;Files=$files;NativeFiles=$nativeFiles;Shortcut=$shortcut} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'install-result.json') -Encoding UTF8

