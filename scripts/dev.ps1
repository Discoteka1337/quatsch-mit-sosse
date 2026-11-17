[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$previousEnvironment = [Environment]::GetEnvironmentVariables('Process')
$exitCode = 1
$startupMutex = [System.Threading.Mutex]::new($false, 'Local\de.quatschmitsosse.app-dev')
$ownsStartupMutex = $false

Push-Location -LiteralPath "$projectRoot"
try {
    try {
        $ownsStartupMutex = $startupMutex.WaitOne(0)
    } catch [System.Threading.AbandonedMutexException] {
        $ownsStartupMutex = $true
    }
    if (-not $ownsStartupMutex) {
        Write-Host 'Der Entwicklungsmodus läuft bereits. Nutze die App im Tray oder beende die erste Entwicklungssitzung mit Strg+C, bevor du neu startest.'
        exit 0
    }
    . "$PSScriptRoot\import-msvc.ps1"
    Import-MsvcEnvironment
    $env:CARGO_TARGET_DIR = 'C:\qms-target'
    $npmExecutable = (Get-Command -Name 'npm.cmd' -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    & "$npmExecutable" --prefix "app/ui" run tauri -- dev
    $exitCode = $LASTEXITCODE
} catch {
    Write-Host "Entwicklungsstart fehlgeschlagen: $($_.Exception.Message)" -ForegroundColor Red
    $exitCode = 1
} finally {
    if ($ownsStartupMutex) { $startupMutex.ReleaseMutex() }
    $startupMutex.Dispose()
    foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) {
        if (-not $previousEnvironment.Contains($name)) {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
    }
    foreach ($name in $previousEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process')
    }
    Pop-Location
}
exit $exitCode
