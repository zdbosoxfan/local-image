param([switch]$OpenSettings, [switch]$NoOpen, [switch]$ShowStatus)

$ErrorActionPreference = 'Stop'
$connectorDir = $PSScriptRoot
$connectorPython = Join-Path $connectorDir '.venv\Scripts\python.exe'
$comfyDir = 'C:\Users\Owner\ComfyUI-Installs\ComfyUI (1)\ComfyUI'
$comfyPython = Join-Path $comfyDir '.venv\Scripts\python.exe'
$comfyMain = Join-Path $comfyDir 'main.py'
$modelPaths = 'C:\Users\Owner\AppData\Roaming\Comfy Desktop\instance-model-paths\inst-1789675529837.yaml'
$rapidRawExe = 'C:\Users\Owner\Documents\RapidRAW-Remove\RapidRAW.exe'
$braveExe = 'C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe'
$settingsUrl = 'http://127.0.0.1:5000/settings'
$logDir = Join-Path $connectorDir 'logs'
$runId = '{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'), $PID
$runLog = Join-Path $logDir "launcher-$runId.log"
$startupMutex = $null

function Write-RunLog([string]$Message) {
    Add-Content -LiteralPath $runLog -Value ('{0} {1}' -f (Get-Date -Format o), $Message) -Encoding UTF8
    if ($ShowStatus) { Write-Host $Message }
}

function Test-ConnectorServer {
    try {
        $schema = Invoke-RestMethod -Uri 'http://127.0.0.1:5000/openapi.json' -TimeoutSec 3
        return ($schema.info.title -eq 'AI Connector')
    } catch { return $false }
}

function Test-ComfyReady {
    try {
        $stats = Invoke-RestMethod -Uri 'http://127.0.0.1:8189/system_stats' -TimeoutSec 3
        return [bool]$stats.system.comfyui_version
    } catch { return $false }
}

function Test-ServiceProcess([string]$Python, [string]$Port) {
    $portPattern = '(?:^|\s)--port\s+' + [regex]::Escape($Port) + '(?:\s|$)'
    $process = Get-CimInstance Win32_Process -Filter "Name='python.exe'" |
        Where-Object { $_.CommandLine -and $_.CommandLine.Contains($Python) -and $_.CommandLine -match $portPattern } |
        Select-Object -First 1
    return ($null -ne $process)
}

function Invoke-StartupLock([scriptblock]$Action) {
    $ownsMutex = $false
    try {
        Write-RunLog 'Waiting for the service startup lock (maximum 15 seconds).'
        try { $ownsMutex = $startupMutex.WaitOne(15000) }
        catch [Threading.AbandonedMutexException] {
            $ownsMutex = $true
            Write-RunLog 'Recovered the startup lock after an interrupted launcher.'
        }
        if (-not $ownsMutex) { throw 'Another launcher is still starting the AI services. Try the shortcut again in a moment.' }
        & $Action
    } finally {
        if ($ownsMutex) { $startupMutex.ReleaseMutex() }
    }
}

function Wait-Service([string]$Name, [scriptblock]$Ready, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    Write-RunLog "Waiting for $Name (maximum $Seconds seconds)."
    do {
        if (& $Ready) { Write-RunLog "$Name is ready."; return }
        if ((Get-Date) -ge $deadline) { throw "$Name did not become ready within $Seconds seconds. See the launcher and service logs in $logDir." }
        Start-Sleep -Milliseconds 500
    } while ($true)
}

function Start-ServiceProcess([string]$Name, [string]$Python, [string]$Directory, [string[]]$Arguments) {
    if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw "$Name Python is missing: $Python" }
    $stdout = Join-Path $logDir "$Name-$runId.stdout.log"
    $stderr = Join-Path $logDir "$Name-$runId.stderr.log"
    $started = Start-Process -FilePath $Python -WorkingDirectory $Directory -ArgumentList $Arguments `
        -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    Write-RunLog "Started $Name process $($started.Id). Error log: $stderr"
}

try {
    if ($ShowStatus) {
        $Host.UI.RawUI.WindowTitle = 'RapidRAW AI Backend'
        Write-Host 'Starting RapidRAW AI. Please wait for the READY message.' -ForegroundColor Cyan
        Write-Host 'The first startup can take a few minutes.'
        Write-Host ''
    }
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    Write-RunLog "Launcher entered. OpenSettings=$OpenSettings NoOpen=$NoOpen"
    # Open the requested application immediately, even when another click is
    # already starting services. The mutex never guards this destination action.
    if (-not $NoOpen -and -not $OpenSettings) {
        Start-Process -FilePath $rapidRawExe -WorkingDirectory (Split-Path $rapidRawExe) -WindowStyle Normal
        Write-RunLog 'Requested the RapidRAW application window.'
    }
    $startupMutex = [Threading.Mutex]::new($false, 'Local\RapidRAW-AI-Launcher')
    Invoke-StartupLock {
        if (Test-ConnectorServer) { Write-RunLog 'Preserving the existing connector.' }
        elseif (Test-ServiceProcess $connectorPython '5000') { Write-RunLog 'The connector is already starting; waiting for it.' }
        else {
            Start-ServiceProcess 'connector' $connectorPython $connectorDir @(
                '-m', 'uvicorn', 'main:app', '--env-file', '.env', '--host', '127.0.0.1', '--port', '5000'
            )
        }
    }
    Wait-Service 'AI connector' { Test-ConnectorServer } 45
    # Settings need only the connector web server. ComfyUI can load afterwards.
    if (-not $NoOpen -and $OpenSettings) {
        if (Test-Path -LiteralPath $braveExe -PathType Leaf) {
            Start-Process -FilePath $braveExe -ArgumentList @('--new-window', $settingsUrl) -WindowStyle Normal
            Write-RunLog 'Requested the settings page in Brave Origin.'
        } else {
            Start-Process -FilePath $settingsUrl -WindowStyle Normal
            Write-RunLog 'Requested the settings page in the default browser.'
        }
    }
    Invoke-StartupLock {
        if (Test-ComfyReady) { Write-RunLog 'Preserving the existing ComfyUI backend and its jobs.' }
        elseif (Test-ServiceProcess $comfyPython '8189') { Write-RunLog 'ComfyUI is already starting; waiting for it.' }
        else {
            foreach ($requiredPath in @($comfyMain, $modelPaths)) {
                if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) { throw "Required ComfyUI file is missing: $requiredPath" }
            }
            # Start-Process joins native arguments; quote filesystem paths with
            # spaces explicitly so Windows PowerShell 5.1 passes them intact.
            Start-ServiceProcess 'comfyui' $comfyPython $comfyDir @(
                '-s', ('"' + $comfyMain + '"'), '--listen', '127.0.0.1', '--port', '8189',
                '--cuda-device', '0', '--disable-dynamic-vram', '--disable-pinned-memory', '--reserve-vram', '12',
                '--database-url', 'sqlite:///C:/Users/Owner/Documents/RapidRAW-AI-Connector/comfy-user/comfy.db',
                '--user-directory', 'C:\Users\Owner\Documents\RapidRAW-AI-Connector\comfy-user',
                '--max-upload-size', '256',
                '--extra-model-paths-config', ('"' + $modelPaths + '"'),
                '--input-directory', 'C:\Users\Owner\ComfyUI-Shared\input',
                '--output-directory', 'C:\Users\Owner\ComfyUI-Shared\output'
            )
        }
    }
    Wait-Service 'ComfyUI backend' { Test-ComfyReady } 180
    Write-RunLog 'Launch completed successfully. Both AI services are ready.'
    if ($ShowStatus) {
        Write-Host ''
        Write-Host 'READY - RapidRAW AI is running.' -ForegroundColor Green
        Write-Host 'RapidRAW: Settings > AI > Self-Hosted: http://127.0.0.1:5000'
        Write-Host "AI settings: $settingsUrl"
        Write-Host 'This status window closes in 8 seconds. AI keeps running in the background.'
        Start-Sleep -Seconds 8
    }
} catch {
    $message = $_.Exception.Message
    try { Write-RunLog "ERROR: $message" } catch {}
    try { $message | Set-Content -LiteralPath (Join-Path $logDir 'launcher-error.txt') -Encoding UTF8 } catch {}
    try {
        $shell = New-Object -ComObject WScript.Shell
        $null = $shell.Popup("$message`r`n`r`nDetails: $runLog", 30, 'RapidRAW AI could not start', 16)
    } catch {}
    exit 1
} finally {
    if ($null -ne $startupMutex) { $startupMutex.Dispose() }
}
