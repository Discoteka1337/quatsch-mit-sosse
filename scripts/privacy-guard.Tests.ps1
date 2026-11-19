[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$guardScript = Join-Path -Path "$PSScriptRoot" -ChildPath 'privacy-guard.ps1'
if (-not (Test-Path -LiteralPath "$guardScript" -PathType Leaf)) {
    throw 'Der Datenschutz-Wächter fehlt: scripts/privacy-guard.ps1.'
}

$tempPath = [System.IO.Path]::GetTempPath()
$tempRoot = (Resolve-Path -LiteralPath "$tempPath").ProviderPath.TrimEnd('\')
$fixtureName = 'qms privacy tests ß ' + [guid]::NewGuid().ToString('N')
$fixtureRoot = Join-Path -Path "$tempRoot" -ChildPath "$fixtureName"
$createdFixture = $false
$script:caseCount = 0
$script:rejectedCount = 0
$sentinel = 'QMS_SYNTHETIC_SECRET_MUST_NOT_APPEAR'

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Write-FixtureFile {
    param([string]$Root, [string]$RelativePath, [string]$Content)
    $destination = Join-Path -Path "$Root" -ChildPath "$RelativePath"
    $parentDirectory = [System.IO.Path]::GetDirectoryName("$destination")
    New-Item -Path "$parentDirectory" -ItemType Directory -Force | Out-Null
    [System.IO.File]::WriteAllText("$destination", $Content, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-GuardCase {
    param(
        [string]$Name,
        [hashtable]$Files = @{},
        [string[]]$Indexed = @(),
        [string[]]$Utf16Files = @(),
        [hashtable]$WorkingChanges = @{},
        [string[]]$Deleted = @(),
        [ValidateSet('Root', 'Missing', 'Subdirectory')]
        [string]$RepositoryMode = 'Root',
        [string]$ExpectedRule = '',
        [string]$ExpectedPath = '',
        [int]$ExpectedLine = 0
    )

    $script:caseCount++
    $caseRoot = Join-Path -Path "$fixtureRoot" -ChildPath ('fall-' + $script:caseCount)
    New-Item -Path "$caseRoot" -ItemType Directory | Out-Null
    if ($RepositoryMode -ne 'Missing') {
        git init --quiet --initial-branch=main "$caseRoot"
        if ($LASTEXITCODE -ne 0) { throw 'Temporäres Git-Repository konnte nicht angelegt werden.' }
    }
    $invocationRoot = $caseRoot
    if ($RepositoryMode -eq 'Subdirectory') {
        $invocationRoot = Join-Path -Path "$caseRoot" -ChildPath 'unterordner'
        New-Item -Path "$invocationRoot" -ItemType Directory | Out-Null
    }
    foreach ($relativePath in $Files.Keys) {
        Write-FixtureFile -Root "$caseRoot" -RelativePath "$relativePath" -Content $Files[$relativePath]
        if ($relativePath -in $Utf16Files) {
            [System.IO.File]::WriteAllText("$caseRoot\$relativePath", $Files[$relativePath], [System.Text.Encoding]::Unicode)
        }
    }
    foreach ($relativePath in $Indexed) {
        git -C "$caseRoot" -c core.autocrlf=false add --force -- "$relativePath"
        if ($LASTEXITCODE -ne 0) { throw "Synthetische Indexdatei konnte nicht vorbereitet werden: $relativePath" }
    }
    foreach ($relativePath in $WorkingChanges.Keys) {
        Write-FixtureFile -Root "$caseRoot" -RelativePath "$relativePath" -Content $WorkingChanges[$relativePath]
    }
    foreach ($relativePath in $Deleted) {
        Remove-Item -LiteralPath "$caseRoot\$relativePath"
    }

    # Always run outside the fixture to exercise explicit repository selection.
    $output = @(& "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$guardScript" -RepositoryRoot "$invocationRoot" 2>&1)
    $exitCode = $LASTEXITCODE
    $diagnostic = $output -join "`n"
    Assert-Condition (-not $diagnostic.Contains($sentinel)) "Quellinhalt wurde ausgegeben: $Name"
    if ($ExpectedRule) {
        Assert-Condition ($exitCode -eq 1) "Verbotenes Beispiel muss Exitcode 1 liefern: $Name; Ergebnis: $exitCode; $diagnostic"
        Assert-Condition ($diagnostic.Contains($ExpectedRule)) "Erwartete Regel fehlt: $Name; $diagnostic"
        Assert-Condition ($diagnostic.Contains($ExpectedPath)) "Betroffener Pfad fehlt: $Name; $diagnostic"
        if ($ExpectedLine -gt 0) {
            $location = [regex]::Escape($ExpectedPath) + ':' + $ExpectedLine + '\b'
            Assert-Condition ($diagnostic -match $location) "Fundstelle fehlt: $Name; $diagnostic"
        }
        $script:rejectedCount++
        Write-Host "OK: Abgewiesen mit Exitcode 1 [$ExpectedRule] - $Name"
    } else {
        Assert-Condition ($exitCode -eq 0) "Erlaubtes Beispiel muss bestehen: $Name; Ergebnis: $exitCode; $diagnostic"
        Write-Host "OK: Erlaubt - $Name"
    }
}

try {
    New-Item -Path "$fixtureRoot" -ItemType Directory | Out-Null
    $createdFixture = $true

    Invoke-GuardCase -Name 'Leeres Repository mit Leerzeichen und ß im Pfad'
    Invoke-GuardCase -Name 'Ordner ohne eigenes Git-Repository' -RepositoryMode Missing -ExpectedRule 'PRIVACY_SCAN'
    Invoke-GuardCase -Name 'Unterordner ist keine Repository-Wurzel' -RepositoryMode Subdirectory -ExpectedRule 'PRIVACY_SCAN'
    foreach ($relativePath in @(
        'testdata/private/notiz.txt',
        'testdata/private/aufnahme.wav',
        'modell.onnx',
        'modell.gguf',
        'modell.bin',
        'aufnahme.wav',
        'modelle/Größe mit Leerzeichen.ONNX',
        'testdata/public/unterordner/aufnahme.wav',
        'testdata/publicity/aufnahme.wav'
    )) {
        Invoke-GuardCase -Name "Verbotene Indexdatei $relativePath" -Files @{
            '.gitignore' = "testdata/private/`n*.onnx`n*.gguf`n*.bin`n*.wav`n"
            $relativePath = $sentinel
        } -Indexed @($relativePath) -ExpectedRule 'PRIVACY_INDEX' -ExpectedPath $relativePath
    }
    Invoke-GuardCase -Name 'Gelöschte Arbeitskopie bleibt im Index verboten' -Files @{
        'testdata/private/gelöscht.txt' = $sentinel
    } -Indexed @('testdata/private/gelöscht.txt') -Deleted @('testdata/private/gelöscht.txt') `
        -ExpectedRule 'PRIVACY_INDEX' -ExpectedPath 'testdata/private/gelöscht.txt'
    Invoke-GuardCase -Name 'Öffentliche WAV direkt im freigegebenen Ordner' -Files @{
        'testdata/public/synthetisch.wav' = $sentinel
    } -Indexed @('testdata/public/synthetisch.wav')
    Invoke-GuardCase -Name 'Nicht versionierte private und ignorierte Dateien' -Files @{
        '.gitignore' = "testdata/private/`nnode_modules/`ntarget/`napp/ui/dist/`n"
        'testdata/private/aufnahme.wav' = $sentinel
        'testdata/private/geheim.rs' = "sentry::init(); // $sentinel"
        'node_modules/example/index.js' = "import x from 'mixpanel';"
        'target/generated.rs' = 'use reqwest::Client;'
        'app/ui/dist/assets/app.js' = "import x from 'posthog-js';"
    }

    $telemetryCases = @(
        @{ Name = 'Rust-Import'; Path = 'crates/textfx/src/lib.rs'; Content = "// synthetic`nuse sentry::Client; // $sentinel"; Line = 2 }
        @{ Name = 'Rust mit Unterstrich'; Path = 'crates/textfx/src/lib.rs'; Content = "// synthetic`nsentry_tracing::layer(); // $sentinel"; Line = 2 }
        @{ Name = 'JavaScript-Paket'; Path = 'app/ui/src/telemetry.js'; Content = "// synthetic`nimport mixpanel from 'mixpanel-browser'; // $sentinel"; Line = 2 }
        @{ Name = 'TypeScript-Paket'; Path = 'app/ui/src/telemetry.ts'; Content = "// synthetic`nimport posthog from 'posthog-js'; // $sentinel"; Line = 2 }
        @{ Name = 'Svelte-Paket'; Path = 'app/ui/src/Telemetry.svelte'; Content = "<script>`nimport * as sdk from '@sentry/svelte'; // $sentinel`n</script>"; Line = 2 }
        @{ Name = 'Segment-Paket'; Path = 'app/ui/src/analytics.ts'; Content = "// synthetic`nimport { AnalyticsBrowser } from '@segment/analytics-next'; // $sentinel"; Line = 2 }
        @{ Name = 'Segment-Rust-Import'; Path = 'crates/textfx/src/lib.rs'; Content = "// synthetic`nuse segment::Client; // $sentinel"; Line = 2 }
        @{ Name = 'Segment-npm-Abhängigkeit'; Path = 'app/ui/package.json'; Content = "{`n  `"dependencies`": { `"segment`": `"1.0.0`" }`n}"; Line = 2 }
        @{ Name = 'Cargo-Abhängigkeit'; Path = 'crates/textfx/Cargo.toml'; Content = "[dependencies]`nsentry = '0.1' # $sentinel"; Line = 2 }
        @{ Name = 'Cargo-Paketalias'; Path = 'crates/textfx/Cargo.toml'; Content = "[dependencies]`nreporter = { package = 'sentry', version = '0.1' } # $sentinel"; Line = 2 }
        @{ Name = 'npm-Abhängigkeit'; Path = 'app/ui/package.json'; Content = "{`n  `"dependencies`": { `"@sentry/browser`": `"1.0.0`" }`n}"; Line = 2 }
        @{ Name = 'Cargo-Lockdatei'; Path = 'Cargo.lock'; Content = "[[package]]`nname = 'sentry-core'`nversion = '0.1.0'"; Line = 2 }
        @{ Name = 'npm-Lockdatei'; Path = 'app/ui/package-lock.json'; Content = "{`n  `"packages`": { `"node_modules/posthog-js`": { `"version`": `"1.0.0`" } }`n}"; Line = 2 }
        @{ Name = 'Zusätzliches Skript'; Path = 'scripts/unsafe.ps1'; Content = "# synthetic`nImport-Module 'mixpanel' # $sentinel"; Line = 2 }
    )
    foreach ($testCase in $telemetryCases) {
        Invoke-GuardCase -Name $testCase.Name -Files @{ $testCase.Path = $testCase.Content } `
            -ExpectedRule 'PRIVACY_TELEMETRY' -ExpectedPath $testCase.Path -ExpectedLine $testCase.Line
    }
    Invoke-GuardCase -Name 'Gestagtes UTF-16-Skript trotz sauberer Arbeitskopie' -Files @{
        'scripts/example.ps1' = "# synthetic`nImport-Module mixpanel # $sentinel"
    } -Utf16Files @('scripts/example.ps1') -Indexed @('scripts/example.ps1') -WorkingChanges @{
        'scripts/example.ps1' = "Write-Host 'synthetisch'"
    } -ExpectedRule 'PRIVACY_TELEMETRY' -ExpectedPath 'scripts/example.ps1' -ExpectedLine 2

    foreach ($captureCode in @(
        'BitBlt(destination, 0, 0, 1, 1, source, 0, 0, 0);',
        'PrintWindow(window, target, 0);',
        'use windows::Graphics::Capture::GraphicsCaptureItem;',
        'use windows::Graphics::{Capture::GraphicsCaptureItem};',
        'Windows.Graphics.Capture.GraphicsCaptureItem;',
        'graphics.CopyFromScreen(0, 0, 0, 0, size);'
    )) {
        Invoke-GuardCase -Name "Bildschirmaufnahme $captureCode" -Files @{
            'crates/context/src/lib.rs' = "// synthetic`n$captureCode // $sentinel"
        } -ExpectedRule 'PRIVACY_CAPTURE' -ExpectedPath 'crates/context/src/lib.rs' -ExpectedLine 2
    }
    Invoke-GuardCase -Name 'Windows-Capture-Feature im Cargo-Manifest' -Files @{
        'crates/context/Cargo.toml' = "[dependencies]`nwindows = { version = '0.1', features = ['Graphics_Capture'] }"
    } -ExpectedRule 'PRIVACY_CAPTURE' -ExpectedPath 'crates/context/Cargo.toml' -ExpectedLine 2

    foreach ($httpClient in @('reqwest', 'ureq', 'hyper')) {
        Invoke-GuardCase -Name "HTTP-Client $httpClient außerhalb net.rs" -Files @{
            'crates/models/src/lib.rs' = "// synthetic`nuse ${httpClient}::Client; // $sentinel"
        } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/models/src/lib.rs' -ExpectedLine 2
    }
    foreach ($relativePath in @('crates/models/src/net/mod.rs', 'crates/models/src/net.rs.backup.rs', 'crates/other/src/net.rs', 'Crates/models/src/net.rs')) {
        Invoke-GuardCase -Name "Keine erweiterte HTTP-Ausnahme für $relativePath" -Files @{
            $relativePath = "// synthetic`nuse reqwest::Client; // $sentinel"
        } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath $relativePath -ExpectedLine 2
    }
    Invoke-GuardCase -Name 'Exakt freigegebene Netzwerkmodule und Abhängigkeiten' -Files @{
        'crates/models/src/net.rs' = 'use reqwest::Client;'
        'crates/llm/src/net.rs' = "use ureq::Agent;`nuse hyper::Request;"
        'Cargo.toml' = "[workspace.dependencies]`nreqwest = '0.1'"
        'Cargo.lock' = "[[package]]`nname = 'hyper'`nversion = '0.1'"
    }
    Invoke-GuardCase -Name 'Netzwerkausnahme erlaubt keine Telemetrie' -Files @{
        'crates/models/src/net.rs' = "// synthetic`nsentry::init(); // $sentinel"
    } -ExpectedRule 'PRIVACY_TELEMETRY' -ExpectedPath 'crates/models/src/net.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'Netzwerkausnahme erlaubt keine Bildschirmaufnahme' -Files @{
        'crates/llm/src/net.rs' = "// synthetic`nPrintWindow(window, target, 0); // $sentinel"
    } -ExpectedRule 'PRIVACY_CAPTURE' -ExpectedPath 'crates/llm/src/net.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'Unverdächtige Bezeichner, Dokumentation und Wächterdateien' -Files @{
        'crates/textfx/src/lib.rs' = 'fn split(segment: &str) { let hyperbolic = segment.len(); }'
        'docs/privacy.md' = 'Verbotene Beispiele: sentry::init(), BitBlt(), reqwest::Client.'
        'README.md' = 'Verbotene Beispiele: posthog, PrintWindow, hyper.'
        'scripts/privacy-guard.ps1' = "'sentry', 'posthog', 'BitBlt', 'reqwest'"
        'scripts/privacy-guard.Tests.ps1' = "'sentry', 'posthog', 'BitBlt', 'reqwest'"
    }

    Invoke-GuardCase -Name 'Gestagter Verstoß trotz sauberer Arbeitskopie' -Files @{
        'crates/context/src/lib.rs' = "// synthetic`nPrintWindow(window, target, 0); // $sentinel"
    } -Indexed @('crates/context/src/lib.rs') -WorkingChanges @{
        'crates/context/src/lib.rs' = 'pub fn safe() {}'
    } -ExpectedRule 'PRIVACY_CAPTURE' -ExpectedPath 'crates/context/src/lib.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'Arbeitskopie mit Verstoß trotz sauberem Index' -Files @{
        'crates/context/src/lib.rs' = 'pub fn safe() {}'
    } -Indexed @('crates/context/src/lib.rs') -WorkingChanges @{
        'crates/context/src/lib.rs' = "// synthetic`nPrintWindow(window, target, 0); // $sentinel"
    } -ExpectedRule 'PRIVACY_CAPTURE' -ExpectedPath 'crates/context/src/lib.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'Gestagter Code bleibt nach lokalem Löschen prüfbar' -Files @{
        'crates/context/src/lib.rs' = "// synthetic`nuse reqwest::Client; // $sentinel"
    } -Indexed @('crates/context/src/lib.rs') -Deleted @('crates/context/src/lib.rs') `
        -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/context/src/lib.rs' -ExpectedLine 2

    Invoke-GuardCase -Name 'HTTP-Paketalias in Inline-Cargo-Abhängigkeit' -Files @{
        'crates/models/Cargo.toml' = "[dependencies]`nnet_client = { package = 'reqwest', version = '0.1' }"
        'crates/models/src/lib.rs' = "// synthetic`nuse net_client::Client; // $sentinel"
    } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/models/src/lib.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'HTTP-Paketalias in Cargo-Abhängigkeitstabelle' -Files @{
        'crates/models/Cargo.toml' = "[dependencies.net_client]`npackage = 'reqwest'`nversion = '0.1'"
        'crates/models/src/lib.rs' = "// synthetic`nuse net_client::Client; // $sentinel"
    } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/models/src/lib.rs' -ExpectedLine 2
    foreach ($declaration in @(
        "[dependencies]`nnet-client.package = 'reqwest'`nnet-client.version = '0.1'",
        "dependencies.net-client.package = 'reqwest'`ndependencies.net-client.version = '0.1'",
        "dependencies.net-client = { package = 'reqwest', version = '0.1' }",
        "[dependencies.net-client]`n'package' = 'reqwest'`nversion = '0.1'",
        "[dependencies]`nnet-client = { 'package' = 'reqwest', version = '0.1' }"
    )) {
        Invoke-GuardCase -Name 'HTTP-Paketalias mit gepunkteten oder zitierten TOML-Schlüsseln' -Files @{
            'crates/models/Cargo.toml' = $declaration
            'crates/models/src/lib.rs' = "// synthetic`nuse net_client::Client; // $sentinel"
        } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/models/src/lib.rs' -ExpectedLine 2
    }
    Invoke-GuardCase -Name 'Gestagter gepunkteter HTTP-Paketalias trotz sauberer Arbeitskopie' -Files @{
        'crates/models/Cargo.toml' = "[dependencies]`nnet_client.package = 'reqwest'`nnet_client.version = '0.1'"
        'crates/models/src/lib.rs' = "// synthetic`nuse net_client::Client; // $sentinel"
    } -Indexed @('crates/models/Cargo.toml', 'crates/models/src/lib.rs') -WorkingChanges @{
        'crates/models/Cargo.toml' = '[dependencies]'
        'crates/models/src/lib.rs' = 'pub fn safe() {}'
    } -ExpectedRule 'PRIVACY_HTTP' -ExpectedPath 'crates/models/src/lib.rs' -ExpectedLine 2
    Invoke-GuardCase -Name 'HTTP-Paketalias im erlaubten Netzwerkmodul' -Files @{
        'crates/models/Cargo.toml' = "[dependencies]`nnet_client = { package = 'reqwest', version = '0.1' }"
        'crates/models/src/net.rs' = 'use net_client::Client;'
    }

    Write-Host "Datenschutz-Regressionen bestanden: $script:caseCount Fälle, davon $script:rejectedCount absichtlich verbotene Beispiele mit Exitcode 1."
} finally {
    if ($createdFixture) {
        $resolvedFixture = (Resolve-Path -LiteralPath "$fixtureRoot").ProviderPath
        $fixture = Get-Item -LiteralPath "$resolvedFixture" -Force
        if ([System.IO.Path]::GetDirectoryName("$resolvedFixture") -ine $tempRoot -or
            $fixture.Name -cne $fixtureName -or ($fixture.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw 'Der temporäre Datenschutz-Testpfad liegt nicht regulär direkt unter dem temporären Verzeichnis.'
        }
        Remove-Item -LiteralPath "$resolvedFixture" -Recurse -Force
    }
}
