[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$previousEnvironment = [Environment]::GetEnvironmentVariables('Process')
$previousLocation = (Get-Location).ProviderPath

try {
    . "$PSScriptRoot\import-msvc.ps1"
    $vswherePath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $detectedInstallation = & "$vswherePath" -latest -products '*' -version '[17.0,19.0)' -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $detectedInstallation) {
        throw 'Für den MSVC-Test wird eine lokale VS2022-/VS2026-Installation mit Desktop-C++ benötigt.'
    }

    # Simulate a hosted runner where only the Enterprise edition is installed.
    $mockFunctionPath = "Function:\$vswherePath"
    $realVswhereCommand = Get-Command -Name "$vswherePath" -CommandType Application
    Set-Item -LiteralPath "$mockFunctionPath" -Value {
        # The developer shell also queries vswhere using its own JSON arguments.
        if ('-version' -notin $args) { return & $realVswhereCommand @args }
        $global:LASTEXITCODE = 0
        $productsIndex = [Array]::IndexOf($args, '-products')
        if ($productsIndex -lt 0 -or $args[$productsIndex + 1] -ne '*') {
            throw 'CI-Regression: Die Editionsauswahl muss auch Visual Studio Enterprise zulassen.'
        }
        $versionIndex = [Array]::IndexOf($args, '-version')
        if ($versionIndex -lt 0 -or $args[$versionIndex + 1] -notmatch '^\[([^,]+),([^\)]+)\)$' -or
            [version]'18.11' -lt [version]$Matches[1] -or [version]'18.11' -ge [version]$Matches[2]) {
            throw 'CI-Regression: Der Versionsbereich muss auch Visual Studio 2026 zulassen.'
        }
        return $detectedInstallation
    }.GetNewClosure()
    try {
        Import-MsvcEnvironment
        $env:VisualStudioVersion = '18.0'
        Import-MsvcEnvironment
        if ($env:VisualStudioVersion -ne '18.0') {
            throw 'Eine bereits initialisierte VS2026-Umgebung darf nicht erneut geladen werden.'
        }
    } finally {
        Remove-Item -LiteralPath "$mockFunctionPath"
    }

    # Reproduce a regular terminal without an initialized MSVC environment.
    foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) {
        if ($name -match '^(VSINSTALLDIR|VCINSTALLDIR|VCTools.*|VSCMD.*|__VSCMD.*|DevEnvDir|LIB|LIBPATH|INCLUDE|WindowsSdk.*|WindowsSDK.*|UniversalCRTSdkDir|UCRTVersion|VisualStudioVersion)$') {
            [Environment]::SetEnvironmentVariable($name, $null, 'Process')
        }
    }
    $env:PATH = (($env:PATH -split ';') | Where-Object { $_ -notmatch 'Microsoft Visual Studio' }) -join ';'

    . "$PSScriptRoot\import-msvc.ps1"
    Import-MsvcEnvironment
    if ($env:VisualStudioVersion -notin @('17.0', '18.0') -or $env:VSCMD_ARG_TGT_ARCH -ne 'x64' -or $env:VSCMD_ARG_HOST_ARCH -ne 'x64') {
        throw 'Die Initialisierung muss Visual Studio 2022 oder 2026 für x64-Builds auswählen.'
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
    Write-Host 'MSVC-Regressionstest bestanden: Enterprise-/VS2026-Auswahl, x64, Laufzeitbibliothek, passender Linker, Telemetrie aus und idempotente Initialisierung.'
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
