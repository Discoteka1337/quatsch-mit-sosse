[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot/..").ProviderPath
$vendorRoot = Join-Path -Path "$projectRoot" -ChildPath 'vendor/tauri-runtime-wry'
$sourcePath = Join-Path -Path "$vendorRoot" -ChildPath 'src/lib.rs'

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Read-NormalizedText {
    param([string]$Path)
    [System.IO.File]::ReadAllText("$Path").Replace("`r`n", "`n")
}

function Get-TextHash {
    param([string]$Content)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        ([BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Content)))).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}

$source = Read-NormalizedText -Path "$sourcePath"
$initStart = $source.IndexOf('  fn init(event_loop: EventLoop<Message<T>>) -> Result<Self> {', [StringComparison]::Ordinal)
$initEnd = $source.IndexOf('impl<T: UserEvent> Runtime<T> for Wry<T>', $initStart, [StringComparison]::Ordinal)
Assert-Condition ($initStart -ge 0 -and $initEnd -gt $initStart) 'Die Runtime-Initialisierung muss eindeutig prüfbar bleiben.'
$initialization = $source.Substring($initStart, $initEnd - $initStart)
Assert-Condition ($initialization -notmatch 'webview_version|create_environment|create_webview|GetAvailableCoreWebView2') 'Native Fenster dürfen beim Runtime-Start keine WebView2-SDK-Abfrage oder Browserinitialisierung auslösen.'
Assert-Condition ($source -notmatch 'webview_runtime_installed') 'Das Ergebnis einer vorzeitigen WebView-Abfrage darf nicht mehr im Kontext gespeichert werden.'

$webviewStart = $source.IndexOf('fn create_webview<T: UserEvent>(', [StringComparison]::Ordinal)
Assert-Condition ($webviewStart -ge 0) 'Der bestehende WebView-Erstellungspfad fehlt.'
$webviewCreation = $source.Substring($webviewStart)
Assert-Condition ($webviewCreation -match '\A[\s\S]*?\) -> Result<WebviewWrapper> \{\n  if wry::webview_version\(\)\.is_err\(\) \{') 'Die Versionsprüfung muss erst innerhalb der tatsächlichen WebView-Erstellung stattfinden.'
Assert-Condition ([regex]::Matches($source, 'wry::webview_version\(\)').Count -eq 1) 'Die WebView2-Versionsabfrage darf nur einmal im verzögerten Erstellungspfad vorkommen.'

# Reverse only the three intentional edits, then compare every shipped upstream file.
# This detects dependency drift independently of the installed Cargo registry cache.
$upstreamSource = $source.Replace("  next_webview_event_id: Arc<AtomicU32>,`n", "  next_webview_event_id: Arc<AtomicU32>,`n  webview_runtime_installed: bool,`n")
$upstreamSource = $upstreamSource.Replace("      next_webview_event_id: Default::default(),`n", "      next_webview_event_id: Default::default(),`n      webview_runtime_installed: wry::webview_version().is_ok(),`n")
$upstreamSource = $upstreamSource.Replace('  if wry::webview_version().is_err() {', '  if !context.webview_runtime_installed {')
$inventory = Get-Content -LiteralPath "$vendorRoot/UPSTREAM-CHECKSUMS.json" -Encoding UTF8 -Raw | ConvertFrom-Json
Assert-Condition ($inventory.Package -eq 'tauri-runtime-wry' -and $inventory.Version -eq '2.12.4') 'Die geprüfte Runtime-Version hat sich geändert und benötigt einen neuen Review.'
Assert-Condition ($inventory.ArchiveSha256 -eq '0d9be5e7af9560a4a60cdd441389a066cdd3e7c5d464a006ae34cb276cc78abd') 'Die Herkunft muss dem zuvor gesperrten Registry-Paket entsprechen.'
$expectedPaths = @($inventory.Files.PSObject.Properties.Name)
foreach ($relative in $expectedPaths) {
    $content = if ($relative -eq 'src/lib.rs') { $upstreamSource } else { Read-NormalizedText -Path "$vendorRoot/$relative" }
    $expectedHash = $inventory.Files.PSObject.Properties[$relative].Value
    Assert-Condition ((Get-TextHash -Content $content) -eq $expectedHash) "Ungeprüfte Änderung an der vendorten Abhängigkeit: $relative"
}
foreach ($file in Get-ChildItem -LiteralPath "$vendorRoot" -File -Recurse -Force) {
    $relative = $file.FullName.Substring($vendorRoot.Length + 1).Replace('\', '/')
    Assert-Condition ($relative -in $expectedPaths -or $relative -in @('UPSTREAM-CHECKSUMS.json', 'LOCAL-NOTICE.md')) "Nicht erfasste Datei in der vendorten Abhängigkeit: $relative"
}

# The application must not enter the still available browser path in the framework.
# Check the worktree: older Git-index content belongs to the previous architecture.
$forbiddenAppApi = '\b(?:WebviewWindowBuilder|WebviewBuilder|WebViewBuilder|CreateCoreWebView2Environment(?:WithOptions)?|with_environment|webview_version)\b'
function Test-NativeAppSource {
    param([string]$Content)
    $Content -notmatch $forbiddenAppApi
}
# Test modules and filenames do not establish runtime reachability; scan the whole file.
$sourceAfterTestModule = "#[cfg(test)]`nmod tests {}`npub fn open() { tauri::WebviewWindowBuilder::new(); }"
Assert-Condition (-not (Test-NativeAppSource -Content $sourceAfterTestModule)) 'Browsercode nach einem Testmodul muss erkannt werden.'
Assert-Condition (-not (Test-NativeAppSource -Content 'fn open() { tauri::webview_version(); }')) 'Direkte SDK-Einstiege müssen erkannt werden.'
Assert-Condition (Test-NativeAppSource -Content 'fn open() { WindowBuilder::new(); }') 'Native Fenster dürfen von der Browserregel nicht blockiert werden.'
foreach ($file in Get-ChildItem -LiteralPath "$projectRoot/app/src-tauri/src" -Filter '*.rs' -File -Recurse -Force) {
    $runtimeSource = Read-NormalizedText -Path "$($file.FullName)"
    Assert-Condition (Test-NativeAppSource -Content $runtimeSource) "Die native Anwendung enthält einen verbotenen Browser- oder SDK-Einstieg: $($file.Name)"
}
$config = Get-Content -LiteralPath "$projectRoot/app/src-tauri/tauri.conf.json" -Encoding UTF8 -Raw | ConvertFrom-Json
Assert-Condition (@($config.app.windows).Count -gt 0) 'Die native Fensterkonfiguration fehlt.'
foreach ($window in $config.app.windows) {
    Assert-Condition ($window.create -eq $false) "Automatische Tauri-WebViews müssen ausgeschaltet sein: $($window.label)"
}
$manifest = Read-NormalizedText -Path "$projectRoot/Cargo.toml"
$patch = [regex]::Match($manifest, '(?ms)^\[patch\.crates-io\]\s*\n(?<body>.*?)(?=^\[|\z)')
Assert-Condition ($patch.Success -and $patch.Groups['body'].Value -match '(?m)^\s*tauri-runtime-wry\s*=\s*\{\s*path\s*=\s*["'']vendor/tauri-runtime-wry["'']\s*\}\s*$') 'Cargo muss die geprüfte lokale Runtime verwenden.'
$lockfile = Read-NormalizedText -Path "$projectRoot/Cargo.lock"
$lockedRuntime = [regex]::Matches($lockfile, '(?ms)^\[\[package\]\]\nname = "tauri-runtime-wry"\n(?<fields>.*?)(?=^\[\[package\]\]|\z)')
Assert-Condition ($lockedRuntime.Count -eq 1 -and $lockedRuntime[0].Groups['fields'].Value -match '(?m)^version = "2\.12\.4"$' -and $lockedRuntime[0].Groups['fields'].Value -notmatch '(?m)^source\s*=') 'Cargo.lock muss ausschließlich die lokale Runtime-Version 2.12.4 auflösen.'

Write-Host 'Native Runtime geprüft: keine Browser- oder SDK-Einstiege in der App; kein SDK-Aufruf beim Start; genau drei geprüfte Upstream-Änderungen; Cargo verwendet die lokale Runtime.'
