[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$ExecutablePath = 'C:\qms-target\release\quatsch-mit-sosse.exe'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-QmsProcessKey {
    param($Record)
    '{0}:{1}' -f $Record.Id, $Record.Started
}

function Select-QmsProcessFamily {
    param([object[]]$Snapshot, $Root, [hashtable]$Known)
    $selected = @{}
    $rootKey = Get-QmsProcessKey $Root
    foreach ($entry in $Snapshot) {
        $key = Get-QmsProcessKey $entry
        if ($key -eq $rootKey -or $Known.ContainsKey($key)) { $selected[$entry.Id] = $entry }
        foreach ($ancestor in $Known.Values) {
            if ($ancestor.Record.Id -eq $entry.ParentId -and $ancestor.Ended -gt 0 -and
                $entry.Started -ge $ancestor.Record.Started -and $entry.Started -le $ancestor.Ended) {
                $selected[$entry.Id] = $entry
            }
        }
    }
    do {
        $added = $false
        foreach ($entry in $Snapshot) {
            if (-not $selected.ContainsKey($entry.Id) -and $selected.ContainsKey($entry.ParentId) -and
                $entry.Started -ge $selected[$entry.ParentId].Started) {
                $selected[$entry.Id] = $entry
                $added = $true
            }
        }
    } while ($added)
    $selected.Values
}

function Test-QmsReadyText {
    param([string]$Text)
    $Text -cmatch '(?m)^QMS_READY\r?\n'
}

function Read-QmsReadyLog {
    param([string]$Path)
    $stream = [System.IO.FileStream]::new("$Path", [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    $reader = [System.IO.StreamReader]::new($stream)
    try { $reader.ReadToEnd() } finally { $reader.Dispose() }
}

function Update-QmsCpuCounter {
    param($Entry, [double]$RawCpuSeconds, [long]$MeasurementStartedTicks)
    if (-not $Entry.HasCpuSample -and $MeasurementStartedTicks -gt 0 -and $Entry.Record.Started -lt $MeasurementStartedTicks) {
        $Entry.CpuOffset = $RawCpuSeconds
        $Entry.LateBaseline = $true
    }
    $cpu = $RawCpuSeconds - $Entry.CpuOffset
    if ($cpu -lt $Entry.CpuSeconds) { throw 'Ein CPU-Zähler ist rückläufig.' }
    $Entry.CpuSeconds = $cpu
    $Entry.HasCpuSample = $true
}

function Get-QmsProcessSnapshot {
    foreach ($entry in @(Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, CreationDate, Name)) {
        if ($null -ne $entry.CreationDate) {
            [pscustomobject]@{
                Id = [int]$entry.ProcessId
                ParentId = [int]$entry.ParentProcessId
                Started = $entry.CreationDate.ToUniversalTime().Ticks
                Name = $entry.Name
            }
        }
    }
}

function Get-QmsFamilySample {
    param($Root, [hashtable]$Tracked, [System.Diagnostics.Stopwatch]$Clock, [double]$OriginSeconds, [long]$MeasurementStartedTicks = 0)
    $snapshot = @(Get-QmsProcessSnapshot)
    foreach ($entry in $Tracked.Values) {
        if ($entry.Ended -eq 0 -and $entry.Process.HasExited) { $entry.Ended = $entry.Process.ExitTime.ToUniversalTime().Ticks }
    }
    $family = @(Select-QmsProcessFamily -Snapshot $snapshot -Root $Root -Known $Tracked)
    $processCountersComplete = $true
    foreach ($entry in $family) {
        $key = Get-QmsProcessKey $entry
        if (-not $Tracked.ContainsKey($key)) {
            $process = $null
            try {
                $process = [System.Diagnostics.Process]::GetProcessById($entry.Id)
                # Hold a handle so an exiting process identity cannot be silently reused.
                $null = $process.Handle
                if ([Math]::Abs($process.StartTime.ToUniversalTime().Ticks - $entry.Started) -ge 10) {
                    $processCountersComplete = $false
                    $process.Dispose()
                    continue
                }
                $Tracked[$key] = [pscustomobject]@{ Record = $entry; Process = $process; CpuSeconds = 0.0; CpuOffset = 0.0; HasCpuSample = $false; LateBaseline = $false; Ended = 0L; Exited = $false; FinalCpuUnavailable = $false }
            } catch [System.ArgumentException] {
                $processCountersComplete = $false
                if ($null -ne $process) { $process.Dispose() }
            } catch [System.InvalidOperationException] {
                $processCountersComplete = $false
                if ($null -ne $process) { $process.Dispose() }
            }
        }
    }
    $privateBytes = 0L
    $workingSetBytes = 0L
    $processCount = 0
    foreach ($entry in $Tracked.Values) {
        if ($entry.Exited) { continue }
        $process = $entry.Process
        $process.Refresh()
        try {
            $cpu = $process.TotalProcessorTime.TotalSeconds
            Update-QmsCpuCounter -Entry $entry -RawCpuSeconds $cpu -MeasurementStartedTicks $MeasurementStartedTicks
        } catch [System.InvalidOperationException] {
            if (-not $process.HasExited) { throw }
            $entry.FinalCpuUnavailable = $true
        }
        if ($process.HasExited) {
            $entry.Exited = $true
            $entry.Ended = $process.ExitTime.ToUniversalTime().Ticks
        } else {
            $privateBytes += $process.PrivateMemorySize64
            $workingSetBytes += $process.WorkingSet64
            $processCount++
        }
    }
    $cpuSeconds = 0.0
    foreach ($entry in $Tracked.Values) { $cpuSeconds += $entry.CpuSeconds }
    [pscustomobject]@{
        ElapsedSeconds = $Clock.Elapsed.TotalSeconds - $OriginSeconds
        CpuSeconds = $cpuSeconds
        PrivateBytes = $privateBytes
        WorkingSetBytes = $workingSetBytes
        ProcessCount = $processCount
        ProcessCountersComplete = $processCountersComplete
    }
}

function Get-QmsMeasurementSummary {
    param([object[]]$Samples, [int]$LogicalProcessors, [double]$StartupSeconds, [bool]$CpuComplete = $true)
    if ($Samples.Count -lt 2 -or $LogicalProcessors -lt 1) { throw 'Zu wenige Messwerte oder ungültige CPU-Anzahl.' }
    $duration = $Samples[-1].ElapsedSeconds - $Samples[0].ElapsedSeconds
    $privateIntegral = 0.0
    $workingSetIntegral = 0.0
    $cpuMax = 0.0
    for ($index = 1; $index -lt $Samples.Count; $index++) {
        $previous = $Samples[$index - 1]
        $current = $Samples[$index]
        $seconds = $current.ElapsedSeconds - $previous.ElapsedSeconds
        $cpuDelta = $current.CpuSeconds - $previous.CpuSeconds
        if ($seconds -le 0 -or $cpuDelta -lt 0) { throw 'Ungültige Zeit- oder CPU-Zähler in der Messreihe.' }
        $cpuMax = [Math]::Max($cpuMax, 100 * $cpuDelta / $seconds / $LogicalProcessors)
        $privateIntegral += ($previous.PrivateBytes + $current.PrivateBytes) / 2.0 * $seconds
        $workingSetIntegral += ($previous.WorkingSetBytes + $current.WorkingSetBytes) / 2.0 * $seconds
    }
    $cpuMean = 100 * ($Samples[-1].CpuSeconds - $Samples[0].CpuSeconds) / $duration / $LogicalProcessors
    $privateMax = ($Samples | Measure-Object -Property PrivateBytes -Maximum).Maximum
    $workingSetMax = ($Samples | Measure-Object -Property WorkingSetBytes -Maximum).Maximum
    [pscustomobject]@{
        PrivateBytes = [pscustomobject]@{ Mean = $privateIntegral / $duration; Max = $privateMax }
        WorkingSetBytes = [pscustomobject]@{ Mean = $workingSetIntegral / $duration; Max = $workingSetMax }
        CpuPercent = [pscustomobject]@{ Mean = $cpuMean; Max = $cpuMax }
        ProcessCount = [pscustomobject]@{ Max = ($Samples | Measure-Object -Property ProcessCount -Maximum).Maximum }
        Targets = [pscustomobject]@{
            MemoryLimitBytes = 100000000; CpuLimitPercent = 1; StartupLimitSeconds = 2
            PrivateBytesPassed = $privateMax -lt 100000000
            WorkingSetPassed = $workingSetMax -lt 100000000
            CpuPassed = if ($CpuComplete) { $cpuMean -lt 1 } else { $null }
            StartupPassed = $StartupSeconds -lt 2
        }
    }
}

function Stop-QmsMeasuredApp {
    param([System.Diagnostics.Process]$Process, [System.Threading.EventWaitHandle]$StopEvent, [hashtable]$Tracked)
    $null = $StopEvent.Set()
    if (-not $Process.WaitForExit(15000)) { throw 'Die gestartete App hat die reguläre Beenden-Anforderung nicht innerhalb von 15 Sekunden ausgeführt.' }
    foreach ($entry in $Tracked.Values) {
        if (-not $entry.Process.WaitForExit(10000)) { throw 'Ein mitgemessener Kindprozess läuft nach dem Beenden weiter.' }
    }
    if ($Process.ExitCode -ne 0) { throw "Die gestartete App wurde mit Fehlercode $($Process.ExitCode) beendet." }
    [pscustomobject]@{ Clean = $true; ExitCode = $Process.ExitCode; Method = 'Windows event -> AppHandle.exit(0)' }
}

function Invoke-QmsIdleMeasurement {
    param(
        [Parameter(Mandatory)][string]$ExecutablePath,
        [double]$WarmupSeconds = 60,
        [double]$DurationSeconds = 30,
        [double]$SampleIntervalSeconds = 1,
        [double]$ReadyTimeoutSeconds = 30
    )
    $exe = (Resolve-Path -LiteralPath "$ExecutablePath").ProviderPath
    $processName = [System.IO.Path]::GetFileNameWithoutExtension("$exe")
    if (@(Get-Process -Name "$processName" -ErrorAction SilentlyContinue).Count -gt 0) {
        throw 'Die App läuft bereits. Bitte diese Instanz regulär beenden, bevor die Messung startet.'
    }
    if ($WarmupSeconds -lt 0 -or $DurationSeconds -le 0 -or $SampleIntervalSeconds -le 0 -or $ReadyTimeoutSeconds -le 0) {
        throw 'Die Zeitangaben für die Messung sind ungültig.'
    }
    $temporaryRoot = [System.IO.Path]::GetTempPath().TrimEnd('\')
    $runName = 'qms-idle-' + [guid]::NewGuid().ToString('N')
    $runDirectory = Join-Path -Path "$temporaryRoot" -ChildPath "$runName"
    $logPath = Join-Path -Path "$runDirectory" -ChildPath 'ready.log'
    $stopName = 'Local\de.quatschmitsosse.benchmark-' + [guid]::NewGuid().ToString('N')
    $stopEvent = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $stopName)
    $process = [System.Diagnostics.Process]::new()
    $tracked = @{}
    $started = $false
    $samples = [System.Collections.Generic.List[object]]::new()
    $logicalProcessors = [Environment]::ProcessorCount
    $report = $null
    New-Item -Path "$runDirectory" -ItemType Directory | Out-Null
    try {
        $process.StartInfo.FileName = "$exe"
        $process.StartInfo.WorkingDirectory = [System.IO.Path]::GetDirectoryName("$exe")
        $process.StartInfo.UseShellExecute = $false
        $process.StartInfo.CreateNoWindow = $true
        $process.StartInfo.EnvironmentVariables['QMS_BENCHMARK_LOG'] = "$logPath"
        $process.StartInfo.EnvironmentVariables['QMS_BENCHMARK_STOP_EVENT'] = $stopName
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        $started = $process.Start()
        if (-not $started) { throw 'Die Release-App konnte nicht gestartet werden.' }
        $startupSeconds = $null
        while ($null -eq $startupSeconds) {
            if (Test-Path -LiteralPath "$logPath") {
                $log = Read-QmsReadyLog -Path "$logPath"
                if (Test-QmsReadyText $log) { $startupSeconds = $clock.Elapsed.TotalSeconds; break }
            }
            if ($process.HasExited) { throw 'Die App wurde vor QMS_READY beendet.' }
            if ($clock.Elapsed.TotalSeconds -ge $ReadyTimeoutSeconds) { throw 'Zeitüberschreitung beim Warten auf QMS_READY.' }
            Start-Sleep -Milliseconds 5
        }
        # CIM creation times have microsecond resolution; use CIM consistently as identity.
        $root = @(Get-QmsProcessSnapshot | Where-Object { $_.Id -eq $process.Id })
        if ($root.Count -ne 1) { throw 'Der gestartete App-Prozess ist nicht mehr vorhanden.' }
        $root = $root[0]
        Write-Host ('Bereit nach {0:N3} s. Aufwärmen: {1} s; Messfenster: {2} s.' -f $startupSeconds, $WarmupSeconds, $DurationSeconds)
        $warmupEnd = $startupSeconds + $WarmupSeconds
        while ($clock.Elapsed.TotalSeconds -lt $warmupEnd) {
            if ($process.HasExited) { throw 'Die App wurde während der Aufwärmzeit beendet.' }
            $null = Get-QmsFamilySample -Root $root -Tracked $tracked -Clock $clock -OriginSeconds 0
            $remaining = $warmupEnd - $clock.Elapsed.TotalSeconds
            if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Min(1000, [Math]::Ceiling($remaining * 1000))) }
        }
        Write-Host 'Aufwärmen abgeschlossen; messe den App-Prozessbaum.'
        $baseline = Get-QmsFamilySample -Root $root -Tracked $tracked -Clock $clock -OriginSeconds 0
        $origin = $baseline.ElapsedSeconds
        $measurementStartedTicks = [DateTime]::UtcNow.Ticks
        $baseline.ElapsedSeconds = 0.0
        $samples.Add($baseline)
        while ($samples[$samples.Count - 1].ElapsedSeconds -lt $DurationSeconds) {
            $next = [Math]::Min($DurationSeconds, $samples[$samples.Count - 1].ElapsedSeconds + $SampleIntervalSeconds)
            $remaining = $origin + $next - $clock.Elapsed.TotalSeconds
            if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining * 1000)) }
            if ($process.HasExited) { throw 'Die App wurde während der Messung beendet.' }
            $samples.Add((Get-QmsFamilySample -Root $root -Tracked $tracked -Clock $clock -OriginSeconds $origin -MeasurementStartedTicks $measurementStartedTicks))
        }
        $cpuComplete = @($tracked.Values | Where-Object { $_.FinalCpuUnavailable -or $_.LateBaseline }).Count -eq 0 -and
            @($samples | Where-Object { -not $_.ProcessCountersComplete }).Count -eq 0
        $summary = Get-QmsMeasurementSummary -Samples $samples.ToArray() -LogicalProcessors $logicalProcessors -StartupSeconds $startupSeconds -CpuComplete $cpuComplete
        $report = [pscustomobject]@{
            SchemaVersion = 1
            MeasuredAt = [DateTimeOffset]::Now.ToString('o')
            Build = [pscustomobject]@{ Profile = 'release'; Sha256 = (Get-FileHash -LiteralPath "$exe" -Algorithm SHA256).Hash.ToLowerInvariant() }
            Conditions = [pscustomobject]@{
                ModelLoaded = $false; MainWindowOpen = $false; HotkeyReadiness = 'not_implemented_step_2.1'
                ReadyScope = 'tray_and_event_loop'; MemoryUnit = 'bytes'; DisplayMegabyteBytes = 1000000
                CpuNormalization = 'total_cpu_seconds / wall_seconds / logical_processors * 100'
                MemoryMean = 'time_weighted_trapezoidal'; MemoryTargetBasis = 'maximum_of_simultaneous_process_sums'
                ProcessDiscovery = 'recursive_parent_pid_and_creation_time_with_retained_processes'
                Limitations = @('Polling kann vollständig zwischen Abfragen lebende Prozesse und Nachkommen nie erfasster Eltern übersehen.', 'Working Sets können gemeinsam genutzte Seiten mehrfach zählen.', 'QMS_READY bestätigt in Schritt 0.3 noch keinen implementierten Hotkey.')
            }
            CpuCountersComplete = $cpuComplete
            LogicalProcessors = $logicalProcessors
            WarmupSeconds = $WarmupSeconds
            RequestedMeasurementSeconds = $DurationSeconds
            MeasurementSeconds = $samples[$samples.Count - 1].ElapsedSeconds
            SampleIntervalSeconds = $SampleIntervalSeconds
            ReadyPollMilliseconds = 5
            StartupSeconds = $startupSeconds
            Summary = $summary
            Processes = @($tracked.Values | ForEach-Object {
                [pscustomobject]@{ Id = $_.Record.Id; ParentId = $_.Record.ParentId; Name = $_.Record.Name; FinalCpuUnavailable = $_.FinalCpuUnavailable; LateBaseline = $_.LateBaseline }
            } | Sort-Object Id)
            Samples = $samples.ToArray()
            Shutdown = $null
        }
    } finally {
        try {
            if ($started) {
                $shutdown = Stop-QmsMeasuredApp -Process $process -StopEvent $stopEvent -Tracked $tracked
                if ($null -ne $report) { $report.Shutdown = $shutdown }
            }
        } finally {
            foreach ($entry in $tracked.Values) { $entry.Process.Dispose() }
            $process.Dispose()
            $stopEvent.Dispose()
            $resolved = (Resolve-Path -LiteralPath "$runDirectory").ProviderPath
            $directory = Get-Item -LiteralPath "$resolved" -Force
            if ([System.IO.Path]::GetDirectoryName("$resolved") -ine $temporaryRoot -or $directory.Name -cne $runName -or
                ($directory.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { throw 'Ungültiger temporärer Messpfad.' }
            Remove-Item -LiteralPath "$resolved" -Recurse -Force
        }
    }
    $report
}

function Save-QmsMeasurement {
    param($Report, [string]$Path)
    if (-not $Report.Shutdown.Clean) { throw 'Eine Messung ohne sauberes Prozessende wird nicht als Ergebnis gespeichert.' }
    $json = $Report | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText("$Path", $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
}

if ($MyInvocation.InvocationName -ne '.') {
    $projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
    $previousEnvironment = [Environment]::GetEnvironmentVariables('Process')
    Push-Location -LiteralPath "$projectRoot"
    try {
        if (-not $SkipBuild) {
            if ($ExecutablePath -ne 'C:\qms-target\release\quatsch-mit-sosse.exe') { throw 'Ein eigener Programmpfad benötigt -SkipBuild.' }
            . "$PSScriptRoot\import-msvc.ps1"
            Import-MsvcEnvironment
            $env:CARGO_TARGET_DIR = 'C:\qms-target'
            $env:CARGO_NET_OFFLINE = 'true'
            $npmExecutable = (Get-Command -Name 'npm.cmd' -CommandType Application | Select-Object -First 1).Source
            & "$npmExecutable" --prefix "app/ui" run tauri -- build --no-bundle
            if ($LASTEXITCODE -ne 0) { throw 'Der Release-Build ist fehlgeschlagen.' }
        }
        $report = Invoke-QmsIdleMeasurement -ExecutablePath "$ExecutablePath"
        $benchmarkDirectory = Join-Path -Path "$projectRoot" -ChildPath 'docs/benchmarks'
        New-Item -Path "$benchmarkDirectory" -ItemType Directory -Force | Out-Null
        $date = ([DateTimeOffset]::Parse($report.MeasuredAt)).ToString('yyyy-MM-dd')
        $reportPath = Join-Path -Path "$benchmarkDirectory" -ChildPath "idle-$date.json"
        Save-QmsMeasurement -Report $report -Path "$reportPath"
        @(
            [pscustomobject]@{ Messwert = 'Private Bytes (MB)'; Mittel = [Math]::Round($report.Summary.PrivateBytes.Mean / 1000000, 2); Maximum = [Math]::Round($report.Summary.PrivateBytes.Max / 1000000, 2); Ziel = '< 100'; Erreicht = $report.Summary.Targets.PrivateBytesPassed }
            [pscustomobject]@{ Messwert = 'Working Set (MB)'; Mittel = [Math]::Round($report.Summary.WorkingSetBytes.Mean / 1000000, 2); Maximum = [Math]::Round($report.Summary.WorkingSetBytes.Max / 1000000, 2); Ziel = '< 100'; Erreicht = $report.Summary.Targets.WorkingSetPassed }
            [pscustomobject]@{ Messwert = 'CPU gesamt (%)'; Mittel = [Math]::Round($report.Summary.CpuPercent.Mean, 3); Maximum = [Math]::Round($report.Summary.CpuPercent.Max, 3); Ziel = '< 1'; Erreicht = $report.Summary.Targets.CpuPassed }
            [pscustomobject]@{ Messwert = 'Start bis QMS_READY (s)'; Mittel = [Math]::Round($report.StartupSeconds, 3); Maximum = '-'; Ziel = '< 2'; Erreicht = $report.Summary.Targets.StartupPassed }
        ) | Format-Table -AutoSize | Out-Host
        Write-Host "JSON gespeichert: $reportPath"
        if (-not $report.CpuCountersComplete) { Write-Warning 'CPU-Zähler unvollständig: CPU-Werte sind Untergrenzen; der CPU-Zielvergleich bleibt unbekannt.' }
        Write-Host 'Gerüst-Benchmark: Hotkey noch nicht implementiert (Schritt 2.1). App und erfasste Kindprozesse wurden sauber beendet.'
    } finally {
        foreach ($name in @([Environment]::GetEnvironmentVariables('Process').Keys)) {
            if (-not $previousEnvironment.Contains($name)) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
        }
        foreach ($name in $previousEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process') }
        Pop-Location
    }
}
