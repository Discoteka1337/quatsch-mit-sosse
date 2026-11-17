[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$fixtureName = '.qms-dev-tests-' + [guid]::NewGuid().ToString('N')
$fixtureRoot = Join-Path -Path "$projectRoot" -ChildPath "$fixtureName"
$stubPath = Join-Path -Path "$fixtureRoot" -ChildPath 'npm.cmd'
$firstRun = $null
$secondRun = $null

try {
    New-Item -Path "$fixtureRoot" -ItemType Directory | Out-Null
    foreach ($expectedExitCode in @(0, 23)) {
        $stub = @"
@echo off
if not "%~1"=="--prefix" exit /b 91
if not "%~2"=="app/ui" exit /b 92
if not "%~3"=="run" exit /b 93
if not "%~4"=="tauri" exit /b 94
if not "%~5"=="--" exit /b 95
if not "%~6"=="dev" exit /b 96
if not "%CARGO_TARGET_DIR%"=="C:\qms-target" exit /b 97
if not "%VSCMD_ARG_TGT_ARCH%"=="x64" exit /b 98
if not "%VSCMD_SKIP_SENDTELEMETRY%"=="1" exit /b 99
if not exist "scripts\dev.ps1" exit /b 100
exit /b $expectedExitCode
"@
        [System.IO.File]::WriteAllText("$stubPath", $stub, [System.Text.Encoding]::ASCII)
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = "$PSHOME\powershell.exe"
        $startInfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$PSScriptRoot\dev.ps1`""
        $startInfo.WorkingDirectory = "$fixtureRoot"
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $startInfo.EnvironmentVariables['PATH'] = "$fixtureRoot;" + $startInfo.EnvironmentVariables['PATH']
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $startInfo
        try {
            if (-not $process.Start()) { throw 'Der Test des Entwicklungsstarts konnte nicht gestartet werden.' }
            $stdoutTask = $process.StandardOutput.ReadToEndAsync()
            $stderrTask = $process.StandardError.ReadToEndAsync()
            $process.WaitForExit()
            $stdout = $stdoutTask.GetAwaiter().GetResult()
            $stderr = $stderrTask.GetAwaiter().GetResult()
            if ($process.ExitCode -ne $expectedExitCode) {
                throw "Der Entwicklungsstart muss Argumente und Exitcode erhalten: erwartet $expectedExitCode, erhalten $($process.ExitCode). $stdout $stderr"
            }
        } finally {
            $process.Dispose()
        }
    }
    # Hold the first real launcher in its npm child, then invoke the launcher again.
    $stub = @'
@echo off
if exist "%~dp0started.txt" exit /b 87
type nul > "%~dp0started.txt"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0wait.ps1"
exit /b %ERRORLEVEL%
'@
    $waitScript = @'
$deadline = [DateTime]::UtcNow.AddSeconds(20)
while (-not (Test-Path -LiteralPath "$PSScriptRoot\release.txt")) {
    if ([DateTime]::UtcNow -gt $deadline) { exit 88 }
    Start-Sleep -Milliseconds 100
}
exit 0
'@
    [System.IO.File]::WriteAllText("$stubPath", $stub, [System.Text.Encoding]::ASCII)
    [System.IO.File]::WriteAllText("$fixtureRoot\wait.ps1", $waitScript, [System.Text.Encoding]::ASCII)
    $firstRun = [System.Diagnostics.Process]::new()
    $firstRun.StartInfo = $startInfo
    if (-not $firstRun.Start()) { throw 'Der erste parallele Entwicklungsstart konnte nicht gestartet werden.' }
    $firstStdout = $firstRun.StandardOutput.ReadToEndAsync()
    $firstStderr = $firstRun.StandardError.ReadToEndAsync()
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath "$fixtureRoot\started.txt")) {
        if ($firstRun.HasExited -or [DateTime]::UtcNow -gt $deadline) {
            throw 'Der erste Entwicklungsstart hat den npm-Aufruf nicht erreicht.'
        }
        Start-Sleep -Milliseconds 100
    }

    $secondRun = [System.Diagnostics.Process]::new()
    $secondRun.StartInfo = $startInfo
    if (-not $secondRun.Start()) { throw 'Der zweite parallele Entwicklungsstart konnte nicht gestartet werden.' }
    $secondStdout = $secondRun.StandardOutput.ReadToEndAsync()
    $secondStderr = $secondRun.StandardError.ReadToEndAsync()
    if (-not $secondRun.WaitForExit(10000)) { throw 'Ein zweiter Entwicklungsstart darf nicht warten oder weitere Werkzeuge starten.' }
    $duplicateOutput = $secondStdout.GetAwaiter().GetResult()
    $duplicateError = $secondStderr.GetAwaiter().GetResult()
    if ($secondRun.ExitCode -ne 0 -or $duplicateOutput -notmatch 'Der Entwicklungsmodus .* bereits') {
        throw "Ein zweiter Entwicklungsstart muss ohne weiteren npm-Aufruf mit Hinweis enden. Exitcode: $($secondRun.ExitCode). $duplicateOutput $duplicateError"
    }
    [System.IO.File]::WriteAllText("$fixtureRoot\release.txt", '')
    if (-not $firstRun.WaitForExit(10000) -or $firstRun.ExitCode -ne 0) {
        throw 'Der erste Entwicklungsstart wurde durch den zweiten Start beeinträchtigt.'
    }
    $null = $firstStdout.GetAwaiter().GetResult()
    $null = $firstStderr.GetAwaiter().GetResult()
    Write-Host 'Entwicklungsstart getestet: Projektpfad, npm-Argumente, MSVC, Rust-Zielordner, Exitcodes und Schutz vor parallelem Start.'
} finally {
    if (Test-Path -LiteralPath "$fixtureRoot") {
        [System.IO.File]::WriteAllText("$fixtureRoot\release.txt", '')
    }
    foreach ($runningProcess in @($secondRun, $firstRun)) {
        if ($null -ne $runningProcess) {
            if (-not $runningProcess.HasExited -and -not $runningProcess.WaitForExit(10000)) {
                $runningProcess.Kill()
                $runningProcess.WaitForExit()
            }
            $runningProcess.Dispose()
        }
    }
    foreach ($file in @('wait.ps1', 'started.txt', 'release.txt')) {
        if (Test-Path -LiteralPath "$fixtureRoot\$file") { Remove-Item -LiteralPath "$fixtureRoot\$file" }
    }
    if (Test-Path -LiteralPath "$stubPath") { Remove-Item -LiteralPath "$stubPath" }
    if (Test-Path -LiteralPath "$fixtureRoot") { Remove-Item -LiteralPath "$fixtureRoot" }
}
