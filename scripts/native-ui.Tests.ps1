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
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW")] private static extern IntPtr ReadControl(IntPtr window, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent, int id);
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")] public static extern int GetWindowLong(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool GetWindowDisplayAffinity(IntPtr window, out uint affinity);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string title);
    [StructLayout(LayoutKind.Sequential)]
    private struct Rect { public int left, top, right, bottom; }
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out Rect rect);
    public static bool Click(IntPtr button) {
        UIntPtr ignored;
        return ReadControl(button, 0x00F5, IntPtr.Zero, IntPtr.Zero, 2, 5000, out ignored) != IntPtr.Zero;
    }
    public static bool NotifyClick(IntPtr parent, int id, IntPtr button) {
        // A background test process cannot guarantee activation after reopening.
        // Deliver the native BN_CLICKED from the verified new control to its owner.
        UIntPtr ignored;
        return button != IntPtr.Zero && GetDlgItem(parent, id) == button
            && ReadControl(parent, 0x0111, new IntPtr(id), button, 2, 5000, out ignored) != IntPtr.Zero;
    }
    public static uint SelectedValue(IntPtr parent, int id, uint message) {
        UIntPtr value;
        var control = GetDlgItem(parent, id);
        if (control == IntPtr.Zero || ReadControl(control, message, IntPtr.Zero, IntPtr.Zero, 2, 1000, out value) == IntPtr.Zero)
            throw new InvalidOperationException("Der gespeicherte Wert ist am Steuerelement nicht lesbar.");
        return value.ToUInt32();
    }
    public static string Position(IntPtr window) {
        Rect rect;
        if (!GetWindowRect(window, out rect)) throw new InvalidOperationException("Fensterposition nicht lesbar.");
        return rect.left + "," + rect.top;
    }
    public static bool Drag(IntPtr window) {
        // Send only client messages to our test window. Leave the real pointer in place.
        var down = new IntPtr((18 << 16) | 80);
        var move = new IntPtr((-30 << 16) | 49);
        return PostMessage(window, 0x0201, new IntPtr(1), down)
            && PostMessage(window, 0x0200, new IntPtr(1), move)
            && PostMessage(window, 0x0202, IntPtr.Zero, down);
    }
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
    $info.EnvironmentVariables['LOCALAPPDATA'] = $testData
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

function Start-NativeSession {
    $script:nativeProcess = Start-NativeTestProcess -Log "$logPath" -StopEvent $stopName
    $rootRecords = @(Get-QmsProcessSnapshot | Where-Object { $_.Id -eq $script:nativeProcess.Id })
    if ($rootRecords.Count -ne 1) { throw 'Der gestartete App-Prozess fehlt in der Prozessliste.' }
    $script:nativeRoot = $rootRecords[0]
    Wait-NativeCondition {
        (Test-Path -LiteralPath "$logPath") -and (Test-QmsReadyText (Read-QmsReadyLog -Path "$logPath"))
    } 'Die App muss QMS_READY melden.'
}

$runId = [guid]::NewGuid().ToString('N')
$testData = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath "qms-overlay-$runId"
$logPath = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath "qms-native-$runId.log"
$stopName = "Local\de.quatschmitsosse.native-test-$runId"
$stopEvent = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $stopName)
$script:nativeProcess = $null
$script:nativeTracked = @{}
$script:nativeClock = [System.Diagnostics.Stopwatch]::StartNew()
$cleanExit = $false
try {
    Start-NativeSession

    $initialWindows = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName))
    if ($initialWindows.Count -ne 2) { throw 'Beim Start müssen genau zwei native App-Fenster vorhanden sein.' }
    $overlay = @($initialWindows | Where-Object { ([QmsNativeWindows]::GetWindowLong($_, -20) -band 0x08000000) -ne 0 })
    if ($overlay.Count -ne 1) { throw 'Genau ein Fenster muss WS_EX_NOACTIVATE verwenden.' }
    if (@($initialWindows | Where-Object { [QmsNativeWindows]::IsWindowVisible($_) }).Count -ne 0) {
        throw 'Die App-Fenster müssen anfangs unsichtbar sein.'
    }
    $initialTexts = @($initialWindows | ForEach-Object { [QmsNativeWindows]::StaticTexts($_, $script:nativeProcess.Id) })
    foreach ($expected in @($locale.generalTitle, $locale.generalDictation, $locale.generalMicrophone, $locale.generalModel)) {
        if ($initialTexts -cnotcontains $expected) { throw 'Auf der Seite Allgemein fehlt ein deutsches Beschriftungsfeld.' }
    }

    for ($cycle = 0; $cycle -lt 3; $cycle++) {
        Open-NativeSettings
        $visible = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName) | Where-Object {
            [QmsNativeWindows]::IsWindowVisible($_)
        })[0]
        $texts = @([QmsNativeWindows]::StaticTexts($visible, $script:nativeProcess.Id))
        foreach ($expected in @($locale.generalTitle, $locale.generalDictation, $locale.generalMicrophone, $locale.generalModel)) {
            if ($texts -cnotcontains $expected) { throw 'Die neu geöffneten Einstellungen enthalten nicht alle nativen deutschen Texte.' }
        }
        $ready = [QmsNativeWindows]::FindWindowEx($visible, [IntPtr]::Zero, 'Button', $locale.overlayShowReady)
        $privacy = [QmsNativeWindows]::FindWindowEx($visible, [IntPtr]::Zero, 'Button', $locale.overlayHideFromCapture)
        if ($ready -eq [IntPtr]::Zero -or $privacy -eq [IntPtr]::Zero) { throw 'Die Overlay-Optionen fehlen.' }
        if ($cycle -eq 0) {
            $tones = [QmsNativeWindows]::FindWindowEx($visible, [IntPtr]::Zero, 'Button', $locale.generalTones)
            if ($tones -eq [IntPtr]::Zero) { throw 'Der Schalter für die Diktattöne fehlt.' }
            $settingsFile = Join-Path -Path "$testData" -ChildPath 'QuatschMitSosse\data\settings.json'
            if (-not [QmsNativeWindows]::Click($tones)) { throw 'Der Tonschalter konnte nicht betätigt werden.' }
            Wait-NativeCondition {
                if (-not (Test-Path -LiteralPath "$settingsFile")) { return $false }
                $saved = Get-Content -LiteralPath "$settingsFile" -Encoding UTF8 -Raw | ConvertFrom-Json
                return $saved.tones -eq $false
            } 'Das Ausschalten der Töne muss im isolierten Benutzerprofil gespeichert werden.'
            # Send only window messages to this isolated app. Never inject global input.
            $hotkey = [QmsNativeWindows]::GetDlgItem($visible, 101)
            if ($hotkey -eq [IntPtr]::Zero) { throw 'Das Aufzeichnen-Feld fehlt.' }
            if (-not [QmsNativeWindows]::Click($hotkey)) { throw 'Das Aufzeichnen-Feld konnte nicht betätigt werden.' }
            foreach ($event in @(@(0x0100, 0x11), @(0x0100, 0x79), @(0x0101, 0x79), @(0x0101, 0x11))) {
                $null = [QmsNativeWindows]::PostMessage($hotkey, $event[0], [IntPtr]$event[1], [IntPtr]::Zero)
            }
            foreach ($choice in @(@(103, 1), @(110, 47))) {
                $combo = [QmsNativeWindows]::GetDlgItem($visible, $choice[0])
                if ($combo -eq [IntPtr]::Zero) { throw 'Die Liste für Modus oder Lautstärke fehlt.' }
                $null = [QmsNativeWindows]::PostMessage($combo, 0x014E, [IntPtr]$choice[1], [IntPtr]::Zero)
                $null = [QmsNativeWindows]::PostMessage($visible, 0x0111, [IntPtr](0x10000 + $choice[0]), $combo)
            }
            Wait-NativeCondition {
                $choices = Get-Content -LiteralPath "$settingsFile" -Encoding UTF8 -Raw | ConvertFrom-Json
                return ($choices.dictationHotkey -ceq 'Strg+F10' -and $choices.mode -ceq 'toggle' -and $choices.volumePercent -eq 47)
            } 'Aufgezeichnetes Kürzel, Umschaltmodus und 47 Prozent müssen gespeichert sein.'
            foreach ($control in @($privacy, $ready)) {
                if (-not [QmsNativeWindows]::Click($control)) { throw 'Die Overlay-Option konnte nicht betätigt werden.' }
            }
            Wait-NativeCondition { [QmsNativeWindows]::IsWindowVisible($overlay[0]) } 'Der Bereit-Punkt muss erscheinen.'
            if ([QmsNativeWindows]::GetForegroundWindow() -eq $overlay[0]) { throw 'Das Overlay hat den Fokus übernommen.' }
            [uint32]$affinity = 0
            if (-not [QmsNativeWindows]::GetWindowDisplayAffinity($overlay[0], [ref]$affinity) -or $affinity -ne 0x11) {
                throw 'Der Aufnahmeschutz ist am sichtbaren Overlay nicht wirksam.'
            }
            $before = [QmsNativeWindows]::Position($overlay[0])
            if (-not [QmsNativeWindows]::Drag($overlay[0])) { throw 'Das Overlay lässt sich nicht ziehen.' }
            Wait-NativeCondition { [QmsNativeWindows]::Position($overlay[0]) -ne $before } 'Das Overlay muss sich beim Ziehen bewegen.'
            $dragged = [QmsNativeWindows]::Position($overlay[0])
            if ([QmsNativeWindows]::GetForegroundWindow() -eq $overlay[0]) { throw 'Beim Ziehen ist der Fokus ins Overlay gewechselt.' }
            if (-not [QmsNativeWindows]::Click($ready)) { throw 'Der Bereit-Punkt konnte nicht ausgeschaltet werden.' }
            Wait-NativeCondition { -not [QmsNativeWindows]::IsWindowVisible($overlay[0]) } 'Der Bereit-Punkt muss verschwinden.'
        } elseif ($cycle -eq 2) {
            if ([QmsNativeWindows]::SelectedValue($visible, 113, 0x00F0) -ne 0) {
                throw 'Die wieder geöffnete Seite muss den ausgeschalteten Bereit-Punkt anzeigen.'
            }
            if (-not [QmsNativeWindows]::NotifyClick($visible, 113, $ready)) { throw 'Das neue Einstellungsfenster konnte die Betätigung des Bereitschalters nicht verarbeiten.' }
            try {
                Wait-NativeCondition { [QmsNativeWindows]::IsWindowVisible($overlay[0]) } 'Der Bereit-Punkt muss erneut erscheinen.'
            } catch {
                $prefs = (Get-Content -LiteralPath "$settingsFile" -Encoding UTF8 -Raw | ConvertFrom-Json).overlay
                Write-Host "Bereit-Punkt: Häkchen $([QmsNativeWindows]::SelectedValue($visible, 113, 0x00F0)) (erwartet 1), gespeichert $($prefs.showReady) (erwartet True)."
                throw
            }
            if ([QmsNativeWindows]::SelectedValue($visible, 113, 0x00F0) -ne 1) {
                throw 'Das Häkchen für den eingeschalteten Bereit-Punkt fehlt.'
            }
        }
        if (-not [QmsNativeWindows]::PostMessage($visible, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
            throw 'Das eigene Einstellungsfenster konnte nicht geschlossen werden.'
        }
        Wait-NativeCondition { -not [QmsNativeWindows]::IsWindow($visible) } 'Das Einstellungsfenster muss beim Schließen zerstört werden.'
        if (@([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName)).Count -ne 1) {
            throw 'Nach dem Schließen darf nur das Overlay verbleiben.'
        }
    }
    Assert-NoBrowserRuntime
    $null = Stop-QmsMeasuredApp -Process $script:nativeProcess -StopEvent $stopEvent -Tracked $script:nativeTracked
    foreach ($entry in $script:nativeTracked.Values) { $entry.Process.Dispose() }
    $script:nativeTracked = @{}
    $script:nativeProcess.Dispose()
    $script:nativeProcess = $null
    $null = $stopEvent.Reset()
    Remove-Item -LiteralPath "$logPath"
    Start-NativeSession
    $restored = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName) | Where-Object { [QmsNativeWindows]::IsWindowVisible($_) })
    if ($restored.Count -ne 1 -or [QmsNativeWindows]::Position($restored[0]) -ne $dragged -or
        -not [QmsNativeWindows]::GetWindowDisplayAffinity($restored[0], [ref]$affinity) -or $affinity -ne 0x11) {
        throw 'Bereit-Punkt, verschobene Position und Aufnahmeschutz müssen den Neustart überstehen.'
    }
    $settingsWindow = @([QmsNativeWindows]::AppWindows($script:nativeProcess.Id, $locale.appName) | Where-Object { $_ -ne $restored[0] })[0]
    if ([QmsNativeWindows]::FindWindowEx($settingsWindow, [IntPtr]::Zero, 'Button', 'Strg+F10') -eq [IntPtr]::Zero -or
        [QmsNativeWindows]::SelectedValue($settingsWindow, 103, 0x0147) -ne 1 -or
        [QmsNativeWindows]::SelectedValue($settingsWindow, 110, 0x0147) -ne 47 -or
        [QmsNativeWindows]::SelectedValue($settingsWindow, 111, 0x00F0) -ne 0) {
        throw 'Kürzel, Umschaltmodus, Lautstärke und Tonauswahl müssen nach dem Neustart im Fenster erscheinen.'
    }
    $null = Stop-QmsMeasuredApp -Process $script:nativeProcess -StopEvent $stopEvent -Tracked $script:nativeTracked
    $cleanExit = $true
    Write-Host 'Native Fensterprüfung bestanden: deutsche Bedienung, Ein-Instanz-Betrieb, Fokus beim Ziehen, Aufnahmeschutz, gespeicherte Position nach Neustart und Prozessprüfung ohne WebView2.'
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
        if (Test-Path -LiteralPath "$testData") {
            $resolvedData = (Resolve-Path -LiteralPath "$testData").ProviderPath
            $expectedData = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "qms-overlay-$runId")
            if ($resolvedData -ne $expectedData) { throw 'Unerwarteter Pfad für die Testdaten.' }
            Remove-Item -LiteralPath "$resolvedData" -Recurse -Force
        }
    }
}
