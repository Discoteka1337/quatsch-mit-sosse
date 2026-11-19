[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\measure-idle.ps1"

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function New-ProcessRecord {
    param([int]$Id, [int]$ParentId, [long]$Started)
    [pscustomobject]@{ Id = $Id; ParentId = $ParentId; Started = $Started; Name = 'synthetic.exe' }
}

$root = New-ProcessRecord 100 1 10
$known = @{}
$family = @(Select-QmsProcessFamily -Snapshot @(
    (New-ProcessRecord 300 200 30), (New-ProcessRecord 900 1 5),
    (New-ProcessRecord 200 100 20), $root
) -Root $root -Known $known)
Assert-Condition (($family.Id | Sort-Object) -join ',' -eq '100,200,300') 'Der ganze unsortierte Prozessbaum muss ohne fremde Prozesse erkannt werden.'
foreach ($entry in $family) { $known[(Get-QmsProcessKey $entry)] = [pscustomobject]@{ Record = $entry; Ended = 0L } }
$known['200:20'].Ended = 35L
$family = @(Select-QmsProcessFamily -Snapshot @(
    $root, (New-ProcessRecord 200 1 40), (New-ProcessRecord 300 200 30),
    (New-ProcessRecord 400 300 50), (New-ProcessRecord 500 200 60), (New-ProcessRecord 350 200 25)
) -Root $root -Known $known)
Assert-Condition (($family.Id | Sort-Object) -join ',' -eq '100,300,350,400') 'Verwaiste und spät erkannte Kinder bleiben enthalten; wiederverwendete Eltern-PIDs und deren neue Kinder müssen ausgeschlossen bleiben.'

Assert-Condition (-not (Test-QmsReadyText 'QMS_READY')) 'Eine Teilzeile darf den Start nicht bestätigen.'
Assert-Condition (-not (Test-QmsReadyText "PREFIX_QMS_READY`n")) 'Ein Teilstring darf den Start nicht bestätigen.'
Assert-Condition (Test-QmsReadyText "QMS_READY`n") 'Der exakte Marker mit Zeilenende muss erkannt werden.'
Assert-Condition (Test-QmsReadyText "QMS_READY`r`n") 'Windows-Zeilenenden müssen erkannt werden.'

$samples = @(
    [pscustomobject]@{ ElapsedSeconds = 0; CpuSeconds = 10; PrivateBytes = 10000000; WorkingSetBytes = 30000000; ProcessCount = 2 }
    [pscustomobject]@{ ElapsedSeconds = 1; CpuSeconds = 10.04; PrivateBytes = 20000000; WorkingSetBytes = 20000000; ProcessCount = 2 }
    [pscustomobject]@{ ElapsedSeconds = 4; CpuSeconds = 10.28; PrivateBytes = 40000000; WorkingSetBytes = 10000000; ProcessCount = 3 }
)
$summary = Get-QmsMeasurementSummary -Samples $samples -LogicalProcessors 4 -StartupSeconds 1.2
Assert-Condition ([Math]::Abs($summary.CpuPercent.Mean - 1.75) -lt 0.00001) 'CPU muss nach echter Messdauer und logischen CPUs normalisiert sein.'
Assert-Condition ([Math]::Abs($summary.CpuPercent.Max - 2) -lt 0.00001) 'CPU-Maximum muss aus einzelnen Intervallen kommen.'
Assert-Condition ($summary.PrivateBytes.Max -eq 40000000) 'Das Maximum muss die gleichzeitige Speichersumme sein.'
Assert-Condition ($summary.PrivateBytes.Mean -eq 26250000) 'Speichermittel müssen zeitgewichtet (Trapezregel) sein.'
Assert-Condition ($summary.Targets.StartupPassed -and -not $summary.Targets.CpuPassed) 'Zielwerte müssen anhand der ungerundeten Werte geprüft werden.'
$samples[2].CpuSeconds = 9
$rejected = $false
try { Get-QmsMeasurementSummary -Samples $samples -LogicalProcessors 4 -StartupSeconds 1.2 | Out-Null } catch { $rejected = $true }
Assert-Condition $rejected 'Rückläufige CPU-Zähler dürfen kein grünes Messergebnis erzeugen.'

$counter = [pscustomobject]@{ Record = (New-ProcessRecord 42 100 10); CpuSeconds = 0.0; CpuOffset = 0.0; HasCpuSample = $false; LateBaseline = $false }
Update-QmsCpuCounter -Entry $counter -RawCpuSeconds 5 -MeasurementStartedTicks 20
Assert-Condition ($counter.CpuSeconds -eq 0 -and $counter.LateBaseline) 'Spät entdeckte ältere Kinder dürfen ihre Anlauf-CPU nicht zum Leerlauf addieren.'
Update-QmsCpuCounter -Entry $counter -RawCpuSeconds 5.5 -MeasurementStartedTicks 20
Assert-Condition ($counter.CpuSeconds -eq 0.5) 'Nach der individuellen Baseline muss weitere CPU-Zeit mitgezählt werden.'
$counter = [pscustomobject]@{ Record = (New-ProcessRecord 43 100 30); CpuSeconds = 0.0; CpuOffset = 0.0; HasCpuSample = $false; LateBaseline = $false }
Update-QmsCpuCounter -Entry $counter -RawCpuSeconds 0.25 -MeasurementStartedTicks 20
Assert-Condition ($counter.CpuSeconds -eq 0.25 -and -not $counter.LateBaseline) 'Im Messfenster geborene Kinder müssen ihre gesamte CPU beitragen.'
$samples[2].CpuSeconds = 10.01
$samples[1].CpuSeconds = 10.005
$summary = Get-QmsMeasurementSummary -Samples $samples -LogicalProcessors 4 -StartupSeconds 1.2 -CpuComplete $false
Assert-Condition ($null -eq $summary.Targets.CpuPassed) 'Unvollständige CPU-Erfassung darf kein bestandenes Ziel melden.'

# A discovered child may exit between the CIM snapshot and opening its process handle.
$snapshotFunction = ${function:Get-QmsProcessSnapshot}
$currentProcess = [System.Diagnostics.Process]::GetCurrentProcess()
$script:vanishingChildRoot = New-ProcessRecord $currentProcess.Id 0 $currentProcess.StartTime.ToUniversalTime().Ticks
$vanishingTracked = @{}
try {
    function Get-QmsProcessSnapshot {
        $script:vanishingChildRoot
        New-ProcessRecord ([int]::MaxValue) $script:vanishingChildRoot.Id ($script:vanishingChildRoot.Started + 1)
    }
    $sample = Get-QmsFamilySample -Root $script:vanishingChildRoot -Tracked $vanishingTracked -Clock ([System.Diagnostics.Stopwatch]::StartNew()) -OriginSeconds 0
    Assert-Condition (-not $sample.ProcessCountersComplete) 'Ein entdecktes, vor dem Handle-Zugriff verschwundenes Kind darf keine vollständige CPU-Erfassung behaupten.'
} finally {
    ${function:Get-QmsProcessSnapshot} = $snapshotFunction
    foreach ($entry in $vanishingTracked.Values) { $entry.Process.Dispose() }
    $currentProcess.Dispose()
}

# A real synthetic GUI executable verifies redirected readiness and cooperative shutdown.
$fixtureName = '.qms-idle-tests-' + [guid]::NewGuid().ToString('N')
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$fixtureRoot = Join-Path -Path "$projectRoot" -ChildPath "$fixtureName"
$previousMode = [Environment]::GetEnvironmentVariable('QMS_IDLE_FIXTURE_MODE')
try {
    New-Item -Path "$fixtureRoot\Pfad mit Leerzeichen ß" -ItemType Directory -Force | Out-Null
    $heldLog = Join-Path -Path "$fixtureRoot" -ChildPath 'held.log'
    $writer = [System.IO.FileStream]::new("$heldLog", [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes("QMS_READY`n")
        $writer.Write($bytes, 0, $bytes.Length)
        $writer.Flush()
        Assert-Condition (Test-QmsReadyText (Read-QmsReadyLog -Path "$heldLog")) 'Ein noch geöffneter App-Logwriter darf die Marker-Lesung nicht blockieren.'
    } finally { $writer.Dispose() }
    $fixtureExe = Join-Path -Path "$fixtureRoot\Pfad mit Leerzeichen ß" -ChildPath 'qms-idle-fixture.exe'
    $source = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
public static class IdleFixture {
    public static void Main(string[] args) {
        using (var stop = EventWaitHandle.OpenExisting(Environment.GetEnvironmentVariable("QMS_BENCHMARK_STOP_EVENT"))) {
            var mode = Environment.GetEnvironmentVariable("QMS_IDLE_FIXTURE_MODE");
            if (args.Length > 0) {
                if (mode == "busy") { while (!stop.WaitOne(0)) { Thread.SpinWait(10000); } }
                else stop.WaitOne();
                return;
            }
            if (mode == "early_exit") return;
            var info = new ProcessStartInfo(Process.GetCurrentProcess().MainModule.FileName, "child");
            info.UseShellExecute = false;
            info.CreateNoWindow = true;
            using (var child = Process.Start(info)) {
                if (mode != "timeout") File.WriteAllText(Environment.GetEnvironmentVariable("QMS_BENCHMARK_LOG"), "QMS_READY\n");
                stop.WaitOne();
                child.WaitForExit();
            }
        }
    }
}
'@
    Add-Type -TypeDefinition $source -OutputAssembly "$fixtureExe" -OutputType WindowsApplication
    $env:QMS_IDLE_FIXTURE_MODE = 'normal'
    $report = Invoke-QmsIdleMeasurement -ExecutablePath "$fixtureExe" -WarmupSeconds 0.15 -DurationSeconds 0.3 -SampleIntervalSeconds 0.1 -ReadyTimeoutSeconds 5
    Assert-Condition ($report.MeasurementSeconds -ge 0.3) 'Das Messfenster darf nicht verkürzt werden.'
    Assert-Condition ($report.Summary.ProcessCount.Max -eq 2) 'Ein echter Kindprozess muss mitgemessen werden.'
    Assert-Condition ($report.Shutdown.Clean -and $report.Shutdown.ExitCode -eq 0) 'App und Kindprozess müssen regulär enden.'
    $jsonPath = Join-Path -Path "$fixtureRoot\Pfad mit Leerzeichen ß" -ChildPath 'idle.json'
    Save-QmsMeasurement -Report $report -Path "$jsonPath"
    $loaded = Get-Content -LiteralPath "$jsonPath" -Encoding UTF8 -Raw | ConvertFrom-Json
    Assert-Condition ($loaded.Samples.Count -ge 2 -and $loaded.Shutdown.Clean) 'JSON muss Messreihe und bestätigtes Prozessende enthalten.'
    try {
        function Get-QmsProcessSnapshot {
            $actual = @(& $snapshotFunction)
            $actual
            $fixtureParent = @($actual | Where-Object { $_.Name -eq 'qms-idle-fixture.exe' } | Select-Object -First 1)
            if ($fixtureParent.Count -eq 1) {
                New-ProcessRecord ([int]::MaxValue) $fixtureParent[0].Id ($fixtureParent[0].Started + 1)
            }
        }
        $incompleteReport = Invoke-QmsIdleMeasurement -ExecutablePath "$fixtureExe" -WarmupSeconds 0.1 -DurationSeconds 0.2 -SampleIntervalSeconds 0.1 -ReadyTimeoutSeconds 5
        Assert-Condition (-not $incompleteReport.CpuCountersComplete -and $null -eq $incompleteReport.Summary.Targets.CpuPassed) 'Ein während der Erfassung verschwundenes Kind muss den CPU-Zielvergleich im Bericht unbekannt lassen.'
    } finally { ${function:Get-QmsProcessSnapshot} = $snapshotFunction }
    $env:QMS_IDLE_FIXTURE_MODE = 'busy'
    $busyReport = Invoke-QmsIdleMeasurement -ExecutablePath "$fixtureExe" -WarmupSeconds 0.1 -DurationSeconds 0.4 -SampleIntervalSeconds 0.1 -ReadyTimeoutSeconds 5
    Assert-Condition ($busyReport.Summary.CpuPercent.Mean -gt 0 -and $busyReport.Samples[-1].CpuSeconds -gt $busyReport.Samples[0].CpuSeconds) 'Ein wirklich rechnender Kindprozess muss auch nach Refresh positive CPU-Deltas liefern.'
    foreach ($mode in @('timeout', 'early_exit')) {
        $env:QMS_IDLE_FIXTURE_MODE = $mode
        $rejected = $false
        try {
            Invoke-QmsIdleMeasurement -ExecutablePath "$fixtureExe" -WarmupSeconds 0.1 -DurationSeconds 0.1 -SampleIntervalSeconds 0.1 -ReadyTimeoutSeconds 0.3 | Out-Null
        } catch { $rejected = $true }
        Assert-Condition $rejected "Ein fehlender Marker muss scheitern: $mode"
        Assert-Condition (@(Get-Process -Name 'qms-idle-fixture' -ErrorAction SilentlyContinue).Count -eq 0) 'Auch der Fehlerpfad muss alle gestarteten Fixture-Prozesse sauber beenden.'
    }
    Write-Host 'Leistungsmessungs-Regressionen bestanden: Prozessbaum, PID-Wiederverwendung, Marker, CPU-/Speicherrechnung, GUI-Start, JSON, Timeout und sauberes Ende.'
} finally {
    [Environment]::SetEnvironmentVariable('QMS_IDLE_FIXTURE_MODE', $previousMode, 'Process')
    if (Test-Path -LiteralPath "$fixtureRoot") {
        $resolved = (Resolve-Path -LiteralPath "$fixtureRoot").ProviderPath
        $fixture = Get-Item -LiteralPath "$resolved" -Force
        if ([System.IO.Path]::GetDirectoryName("$resolved") -ine $projectRoot -or $fixture.Name -cne $fixtureName -or
            ($fixture.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw 'Ungültiger temporärer Testpfad.' }
        Remove-Item -LiteralPath "$resolved" -Recurse -Force
    }
}
