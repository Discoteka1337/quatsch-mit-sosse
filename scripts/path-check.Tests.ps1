[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

& "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\import-msvc.Tests.ps1"
if ($LASTEXITCODE -ne 0) {
    throw 'Regression: Die gemeinsame MSVC-Initialisierung ist fehlgeschlagen.'
}
& "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\dev.Tests.ps1"
if ($LASTEXITCODE -ne 0) {
    throw 'Regression: Der Entwicklungsstart ist fehlgeschlagen.'
}

$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$checkScript = Join-Path -Path "$PSScriptRoot" -ChildPath 'path-check.ps1'
$testDirectory = Join-Path -Path "$projectRoot" -ChildPath 'Test Ordner ß'
if (Test-Path -LiteralPath "$testDirectory") {
    throw "Der Testordner existiert bereits; Regressionstest abgebrochen: $testDirectory"
}

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = "$PSHOME\powershell.exe"
$command = "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); & '{0}'" -f $checkScript.Replace("'", "''")
$encodedCommand = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($command))
$startInfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -OutputFormat Text -EncodedCommand $encodedCommand"
$startInfo.WorkingDirectory = "$projectRoot"
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
$startInfo.StandardErrorEncoding = [System.Text.Encoding]::UTF8

# Reproduce a regular terminal without an initialized MSVC environment.
foreach ($name in @($startInfo.EnvironmentVariables.Keys)) {
    if ($name -match '^(VSINSTALLDIR|VCINSTALLDIR|VCTools.*|VSCMD.*|__VSCMD.*|DevEnvDir|LIB|LIBPATH|INCLUDE|WindowsSdk.*|WindowsSDK.*|UniversalCRTSdkDir|UCRTVersion)$') {
        $startInfo.EnvironmentVariables.Remove($name)
    }
}
$startInfo.EnvironmentVariables['PATH'] = (($startInfo.EnvironmentVariables['PATH'] -split ';') |
    Where-Object { $_ -notmatch 'Microsoft Visual Studio' }) -join ';'

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo
try {
    if (-not $process.Start()) {
        throw 'Der Pfadtest konnte nicht gestartet werden.'
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $result = $process.ExitCode
} finally {
    $process.Dispose()
}

Write-Host $stdout
if ($result -ne 0 -and $stderr) {
    Write-Host $stderr
}
if ($result -ne 0) {
    throw "Regression: Der Pfadtest scheitert im normalen Terminal (Exitcode $result)."
}
if ($stdout -notmatch 'Rust-Build: OK' -or $stdout -notmatch 'npm-Installation: OK') {
    throw 'Rust-Build oder npm-Installation wurde nicht erfolgreich abgeschlossen.'
}
if (Test-Path -LiteralPath "$testDirectory") {
    throw 'Der temporäre Testordner wurde nicht entfernt.'
}

$binaryPath = 'C:\qms-target\debug\qms-path-check.exe'
$hello = & "$binaryPath"
if ($LASTEXITCODE -ne 0 -or $hello -cne 'Hello, world!') {
    throw 'Das gebaute Rust-Programm liefert nicht die erwartete Ausgabe.'
}
Write-Host 'Regressionstest bestanden: Rust und npm laufen ohne vorbereitete Entwicklerumgebung; Hello World startet aus C:\qms-target; der Testordner ist entfernt.'
