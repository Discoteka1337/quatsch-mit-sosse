[CmdletBinding()]
param([string]$ExecutablePath = 'C:\qms-target\debug\quatsch-mit-sosse.exe')

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$nativeExecutable = (Resolve-Path -LiteralPath "$ExecutablePath").ProviderPath
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$locale = Get-Content -LiteralPath "$projectRoot\app\ui\src\locales\de.json" -Encoding UTF8 -Raw | ConvertFrom-Json
. "$PSScriptRoot\measure-idle.ps1" -ExecutablePath "$nativeExecutable"

if (@(Get-Process -Name 'quatsch-mit-sosse' -ErrorAction SilentlyContinue).Count -gt 0) {
    throw 'Die App läuft bereits. Die native Regression darf keine bestehende Sitzung beeinflussen.'
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class QmsNativeWindows {
    private delegate bool WindowCallback(IntPtr window, IntPtr state);
    [DllImport("user32.dll")] private static extern bool EnumWindows(WindowCallback callback, IntPtr state);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr parent, WindowCallback callback, IntPtr state);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam, StringBuilder text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    public static IntPtr[] AppWindows(uint processId, string title) {
        var result = new List<IntPtr>();
        EnumWindows((window, state) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == processId) {
                var text = new StringBuilder(256);
                GetWindowText(window, text, text.Capacity);
                if (text.ToString() == title) result.Add(window);
            }
            return true;
        }, IntPtr.Zero);
        return result.ToArray();
    }
    public static string[] StaticTexts(IntPtr parent, uint processId) {
        var result = new List<string>();
        var failed = false;
        EnumChildWindows(parent, (window, state) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != processId) return true;
            var className = new StringBuilder(256);
            GetClassName(window, className, className.Capacity);
            if (!String.Equals(className.ToString(), "Static", StringComparison.OrdinalIgnoreCase)) return true;
            var text = new StringBuilder(4096);
            UIntPtr length;
            if (SendMessageTimeout(window, 0x000D, new IntPtr(text.Capacity), text, 0x0002, 1000, out length) == IntPtr.Zero) failed = true;
            else result.Add(text.ToString());
            return true;
        }, IntPtr.Zero);
        if (failed) throw new InvalidOperationException("Die nativen Platzhaltertexte sind nicht lesbar.");
        return result.ToArray();
    }
}
'@

function Start-NativeTestProcess {
    param([string]$Log = '', [string]$StopEvent = '')
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName = "$nativeExecutable"
    $info.WorkingDirectory = [System.IO.Path]::GetDirectoryName("$nativeExecutable")
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $info.EnvironmentVariables.Remove('QMS_BENCHMARK_LOG')
    $info.EnvironmentVariables.Remove('QMS_BENCHMARK_STOP_EVENT')
    if ($Log) {
        $info.EnvironmentVariables['QMS_BENCHMARK_LOG'] = $Log
        $info.EnvironmentVariables['QMS_BENCHMARK_STOP_EVENT'] = $StopEvent
    }
    $startedProcess = [System.Diagnostics.Process]::new()
    $startedProcess.StartInfo = $info
    if (-not $startedProcess.Start()) {
        $startedProcess.Dispose()
        throw 'Der App-Prozess für die native Regression konnte nicht gestartet werden.'
    }
    return $startedProcess
}

function Assert-NoBrowserRuntime {
    $sample = Get-QmsFamilySample -Root $script:nativeRoot -Tracked $script:nativeTracked -Clock $script:nativeClock -OriginSeconds 0
    if (@($script:nativeTracked.Values | Where-Object { $_.Record.Name -ieq 'msedgewebview2.exe' }).Count -gt 0) {
        throw 'Datenschutz-Regression: Die App hat einen WebView2-Prozess gestartet.'
    }
    if (-not $sample.ProcessCountersComplete) { throw 'Die Prozessfamilie konnte nicht vollständig geprüft werden.' }
}

function Wait-NativeCondition {
    param([scriptblock]$Condition, [string]$Description)
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        if ($script:nativeProcess.HasExited) { throw 'Die App wurde während der nativen Regression unerwartet beendet.' }
        Assert-NoBrowserRuntime
        if (& $Condition) { return }
        if ($timer.Elapsed.TotalSeconds -gt 25) { throw "Zeitüberschreitung: $Description" }
        Start-Sleep -Milliseconds 30
    }
}

function Open-NativeSettings {
    $second = Start-NativeTestProcess
    try {
        if (-not $second.WaitForExit(15000)) { throw 'Die zweite Instanz hat sich nicht rechtzeitig beendet.' }
        if ($second.ExitCode -ne 0) { throw 'Die zweite Instanz wurde mit einem Fehler beendet.' }
    } finally {
        # This handle belongs only to the process started by this test.
        if (-not $second.HasExited) { $second.Kill(); $second.WaitForExit() }
        $second.Dispose()
    }
    Wait-NativeCondition {
        @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName) | Where-Object {
            [QmsNativeWindows]::IsWindowVisible($_)
        }).Count -eq 1
    } 'Das Einstellungsfenster muss sichtbar werden.'
}

$runId = [guid]::NewGuid().ToString('N')
$logPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath "qms-native-$runId.log"
$stopName = "Local\de.quatschmitsosse.native-test-$runId"
$stopEvent = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $stopName)
$script:nativeProcess = $null
$script:nativeTracked = @{}
$script:nativeClock = [System.Diagnostics.Stopwatch]::StartNew()
$cleanExit = $false
try {
    $script:nativeProcess = Start-NativeTestProcess -Log "$logPath" -StopEvent $stopName
    $rootRecords = @(Get-QmsProcessSnapshot | Where-Object { $_.Id -eq $script:nativeProcess.Id })
    if ($rootRecords.Count -ne 1) { throw 'Der gestartete App-Prozess fehlt in der Prozessliste.' }
    $script:nativeRoot = $rootRecords[0]
    Wait-NativeCondition {
        (Test-Path -LiteralPath "$logPath") -and (Test-QmsReadyText (Read-QmsReadyLog -Path "$logPath"))
    } 'Die App muss QMS_READY melden.'

    $initialWindows = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName))
    if ($initialWindows.Count -ne 2) { throw 'Beim Start müssen genau zwei native App-Fenster vorhanden sein.' }
    if (@($initialWindows | Where-Object { [QmsNativeWindows]::IsWindowVisible($_) }).Count -ne 0) {
        throw 'Die App-Fenster müssen anfangs unsichtbar sein.'
    }
    $initialTexts = @($initialWindows | ForEach-Object { [QmsNativeWindows]::StaticTexts($_, $script:nativeProcess.Id) })
    foreach ($expected in @($locale.appName, $locale.settings, $locale.settingsPlaceholder, $locale.overlayPlaceholder)) {
        if ($initialTexts -cnotcontains $expected) { throw 'Ein nativer Platzhaltertext aus dem deutschen Sprachkatalog fehlt.' }
    }

    for ($cycle = 0; $cycle -lt 3; $cycle++) {
        Open-NativeSettings
        $visible = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName) | Where-Object {
            [QmsNativeWindows]::IsWindowVisible($_)
        })[0]
        $texts = @([QmsNativeWindows]::StaticTexts($visible, $script:nativeProcess.Id))
        foreach ($expected in @($locale.appName, $locale.settings, $locale.settingsPlaceholder)) {
            if ($texts -cnotcontains $expected) { throw 'Die neu geöffneten Einstellungen enthalten nicht alle nativen deutschen Texte.' }
        }
        if (-not [QmsNativeWindows]::PostMessage($visible, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
            throw 'Das eigene Einstellungsfenster konnte nicht geschlossen werden.'
        }
        Wait-NativeCondition { -not [QmsNativeWindows]::IsWindow($visible) } 'Das Einstellungsfenster muss beim Schließen zerstört werden.'
        if (@([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName)).Count -ne 1) {
            throw 'Nach dem Schließen darf nur das unsichtbare Overlay verbleiben.'
        }
    }
    Assert-NoBrowserRuntime
    $null = Stop-QmsMeasuredApp -Process $script:nativeProcess -StopEvent $stopEvent -Tracked $script:nativeTracked
    $cleanExit = $true
    Write-Host 'Native UI-Regression bestanden: keine WebView2-Kinder, zwei versteckte Fenster, deutsche native Texte, Single-instance, drei Zerstörungs-/Neuöffnungszyklen und reguläres Ende.'
} finally {
    try {
        $null = $stopEvent.Set()
        if ($null -ne $script:nativeProcess) {
            try {
                if (-not $cleanExit -and -not $script:nativeProcess.WaitForExit(15000)) {
                    # Bound failure cleanup to the exact process handle created above.
                    $script:nativeProcess.Kill()
                    $script:nativeProcess.WaitForExit()
                }
                foreach ($entry in $script:nativeTracked.Values) {
                    if (-not $entry.Process.HasExited -and -not $entry.Process.WaitForExit(10000)) {
                        throw 'Ein erfasster eigener Kindprozess läuft nach dem Regressionstest weiter.'
                    }
                }
            } finally {
                foreach ($entry in $script:nativeTracked.Values) { $entry.Process.Dispose() }
                $script:nativeProcess.Dispose()
            }
        }
    } finally {
        $stopEvent.Dispose()
        if (Test-Path -LiteralPath "$logPath") { Remove-Item -LiteralPath "$logPath" }
    }
}
