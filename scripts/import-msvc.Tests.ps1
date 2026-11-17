[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$previousEnvironment = [Environment]::GetEnvironmentVariables('Process')
$previousLocation = (Get-Location).ProviderPath

try {
    # Reproduce a regular terminal without an initialized MSVC environment.
    foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) {
        if ($name -match '^(VSINSTALLDIR|VCINSTALLDIR|VCTools.*|VSCMD.*|__VSCMD.*|DevEnvDir|LIB|LIBPATH|INCLUDE|WindowsSdk.*|WindowsSDK.*|UniversalCRTSdkDir|UCRTVersion|VisualStudioVersion)$') {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
    }
    $env:PATH = (($env:PATH -split ';') | Where-Object { $_ -notmatch 'Microsoft Visual Studio' }) -join ';'

    . "$PSScriptRoot\import-msvc.ps1"
    Import-MsvcEnvironment
    if ($env:VisualStudioVersion -ne '17.0' -or $env:VSCMD_ARG_TGT_ARCH -ne 'x64' -or $env:VSCMD_ARG_HOST_ARCH -ne 'x64') {
        throw 'Die Initialisierung muss Visual Studio 2022 für x64-Builds auswählen.'
    }
    if ($env:VSCMD_SKIP_SENDTELEMETRY -ne '1') {
        throw 'Die Initialisierung muss die MSVC-Telemetrie abschalten.'
    }
    $runtimeLibrary = Join-Path -Path "$env:VCToolsInstallDir" -ChildPath 'lib\x64\msvcrt.lib'
    $runtimeDirectory = [System.IO.Path]::GetDirectoryName("$runtimeLibrary")
    if (-not (Test-Path -LiteralPath "$runtimeLibrary" -PathType Leaf) -or
        $runtimeDirectory -notin @($env:LIB -split ';' | ForEach-Object { $_.TrimEnd('\') })) {
        throw 'Die gewählte MSVC-Laufzeit muss vorhanden und im Linker-Suchpfad sein.'
    }
    $expectedLinker = Join-Path -Path "$env:VCToolsInstallDir" -ChildPath 'bin\Hostx64\x64\link.exe'
    $actualLinker = (Get-Command -Name 'link.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    if ($actualLinker -ine $expectedLinker) {
        throw 'Der Linker muss zur initialisierten Visual-Studio-Installation gehören.'
    }

    $initializedEnvironment = [Environment]::GetEnvironmentVariables('Process')
    Import-MsvcEnvironment
    $repeatedEnvironment = [Environment]::GetEnvironmentVariables('Process')
    if ($initializedEnvironment.Count -ne $repeatedEnvironment.Count) {
        throw 'Wiederholtes Initialisieren darf keine weiteren Umgebungsvariablen erzeugen.'
    }
    foreach ($name in $initializedEnvironment.Keys) {
        if ($initializedEnvironment[$name] -cne $repeatedEnvironment[$name]) {
            throw "Wiederholtes Initialisieren darf die Umgebung nicht verändern: $name"
        }
    }
    if ((Get-Location).ProviderPath -cne $previousLocation) {
        throw 'Die MSVC-Initialisierung darf das Arbeitsverzeichnis nicht ändern.'
    }
    Write-Host 'MSVC-Regressionstest bestanden: VS2022 x64, Laufzeitbibliothek, passender Linker, Telemetrie aus und idempotente Initialisierung.'
} finally {
    foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) {
        if (-not $previousEnvironment.Contains($name)) {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
    }
    foreach ($name in $previousEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process')
    }
}
