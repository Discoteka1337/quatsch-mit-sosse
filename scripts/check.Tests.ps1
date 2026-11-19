[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$checkScript = Join-Path -Path "$PSScriptRoot" -ChildPath 'check.ps1'
if (-not (Test-Path -LiteralPath "$checkScript" -PathType Leaf)) {
    throw 'Der Prüflauf für die einzelnen Bauschritte fehlt.'
}

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$fixtureName = '.qms-check-tests-' + [guid]::NewGuid().ToString('N')
$fixtureRoot = Join-Path -Path "$projectRoot" -ChildPath "$fixtureName"
$createdFixture = $false
try {
    New-Item -Path "$fixtureRoot" -ItemType Directory | Out-Null
    $createdFixture = $true
    New-Item -Path "$fixtureRoot\scripts" -ItemType Directory | Out-Null
    Copy-Item -LiteralPath "$checkScript" -Destination "$fixtureRoot\scripts\check.ps1"
    Copy-Item -LiteralPath "$PSScriptRoot\check.Tests.ps1" -Destination "$fixtureRoot\scripts\check.Tests.ps1"
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\import-msvc.ps1", 'throw "PlanOnly darf keine MSVC-Umgebung initialisieren."')
    git init --quiet --initial-branch=main "$fixtureRoot"
    if ($LASTEXITCODE -ne 0) { throw 'Test-Repository konnte nicht angelegt werden.' }

    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'V5 darf ohne Pfad-Regressionstest nicht bestehen.'
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\path-check.Tests.ps1", 'exit 0')

    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
    Assert-Condition $plan.CanPass 'V5 wird durch erst später vorgesehene Dateien blockiert.'
    foreach ($nativeCheck in @('native-runtime', 'native-ui')) {
        Assert-Condition ($plan.Checks.Id -contains $nativeCheck) "Die native Datenschutzkorrektur muss zentral geprüft werden: $nativeCheck"
    }
    Assert-Condition (@($plan.Checks | Where-Object { $_.State -eq 'NotScheduled' }).Count -eq 11) 'Spätere Prüfungen müssen ausdrücklich als noch nicht vorgesehen erscheinen.'
    $measurementCheck = @($plan.Checks | Where-Object { $_.Id -eq 'measurement-rules' })
    Assert-Condition ($measurementCheck.Count -eq 1 -and $measurementCheck[0].Reason -eq 'erst ab Schritt 0.3 vorgesehen') 'Die Messregression muss vor Schritt 0.3 als noch nicht vorgesehen erscheinen.'

    foreach ($measurementPath in @('scripts/measure-idle.ps1', 'scripts/measure-idle.Tests.ps1', 'docs/benchmarks/idle-2026-11-17.json')) {
        $measurementFile = Join-Path -Path "$fixtureRoot" -ChildPath "$measurementPath"
        New-Item -Path "$([System.IO.Path]::GetDirectoryName($measurementFile))" -ItemType Directory -Force | Out-Null
        [System.IO.File]::WriteAllText("$measurementFile", 'synthetic benchmark fixture')
        foreach ($earlyStage in @('V5', '0.1')) {
            $plan = & "$fixtureRoot\scripts\check.ps1" -Stage $earlyStage -PlanOnly
            Assert-Condition (-not $plan.Allowed) "Begonnene Messdatei muss $earlyStage sperren: $measurementPath"
        }
        Remove-Item -LiteralPath "$measurementFile"
    }

    foreach ($unsafePath in @('example.onnx', 'modell-ß.onnx', 'testdata/private/aufnahme-ß.txt')) {
        $unsafeFile = Join-Path -Path "$fixtureRoot" -ChildPath "$unsafePath"
        $parentDirectory = [System.IO.Path]::GetDirectoryName("$unsafeFile")
        New-Item -Path "$parentDirectory" -ItemType Directory -Force | Out-Null
        [System.IO.File]::WriteAllText("$unsafeFile", 'synthetic test')
        git -C "$fixtureRoot" add -- "$unsafePath"
        if ($LASTEXITCODE -ne 0) { throw 'Synthetischer Index-Test konnte nicht vorbereitet werden.' }
        $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
        Assert-Condition (-not $plan.Allowed) "Verbotene Datei wurde nicht blockiert: $unsafePath"
        git -C "$fixtureRoot" rm --quiet --cached -- "$unsafePath"
        if ($LASTEXITCODE -ne 0) { throw 'Synthetischer Index-Test konnte nicht aufgeräumt werden.' }
        Remove-Item -LiteralPath "$unsafeFile"
    }

    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Standardprüfung darf fehlende Projektdateien nicht überspringen.'
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage '0.1' -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Schritt 0.1 darf ohne Cargo-Manifest und UI nicht bestehen.'

    New-Item -Path "$fixtureRoot\app\ui" -ItemType Directory -Force | Out-Null
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Ein begonnenes App-Gerüst muss den Vorbereitungsmodus sperren.'
    [System.IO.File]::WriteAllText("$fixtureRoot\Cargo.toml", '[workspace]')
    [System.IO.File]::WriteAllText("$fixtureRoot\app\ui\package.json", '{}')
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage '0.1' -PlanOnly
    Assert-Condition $plan.CanPass 'Schritt 0.1 darf nicht den Datenschutz-Wächter aus Schritt 0.2 voraussetzen.'
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Vollprüfung muss den Datenschutz-Wächter verlangen.'

    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\privacy-guard.ps1", 'exit 0')
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage '0.1' -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Ein vorhandener Datenschutz-Wächter darf nicht über Schritt 0.1 übersprungen werden.'
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Vollprüfung muss auch die Datenschutz- und CI-Regressionstests verlangen.'
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\privacy-guard.Tests.ps1", 'exit 0')
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\ci.Tests.ps1", 'exit 0')
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Vollprüfung muss den CI-Workflow verlangen.'
    New-Item -Path "$fixtureRoot\.github\workflows" -ItemType Directory -Force | Out-Null
    [System.IO.File]::WriteAllText("$fixtureRoot\.github\workflows\ci.yml", 'name: Test')
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Vollprüfung muss die Messregression und das Messskript verlangen.'
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\measure-idle.ps1", 'exit 0')
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Die Vollprüfung darf die fehlende Messregression nicht überspringen.'
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\measure-idle.Tests.ps1", 'exit 0')
    Remove-Item -LiteralPath "$fixtureRoot\scripts\measure-idle.ps1"
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition (-not $plan.CanPass) 'Eine Messregression ohne das Messskript muss die Vollprüfung blockieren.'
    [System.IO.File]::WriteAllText("$fixtureRoot\scripts\measure-idle.ps1", 'exit 0')
    $nativeRequirements = @('scripts/native-runtime.Tests.ps1', 'scripts/native-ui.Tests.ps1', 'vendor/tauri-runtime-wry/Cargo.toml', 'vendor/tauri-runtime-wry/src/lib.rs')
    foreach ($requiredPath in $nativeRequirements) {
        $requiredFile = Join-Path -Path "$fixtureRoot" -ChildPath "$requiredPath"
        $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
        Assert-Condition (-not $plan.CanPass) "Die native Pflichtvoraussetzung darf nicht fehlen: $requiredPath"
        New-Item -Path "$([System.IO.Path]::GetDirectoryName($requiredFile))" -ItemType Directory -Force | Out-Null
        [System.IO.File]::WriteAllText("$requiredFile", 'synthetic native prerequisite')
    }
    $plan = & "$fixtureRoot\scripts\check.ps1" -PlanOnly
    Assert-Condition $plan.CanPass 'Die Vollprüfung muss alle vorhandenen Voraussetzungen erkennen.'
    Remove-Item -LiteralPath "$fixtureRoot\scripts\privacy-guard.ps1", "$fixtureRoot\app\ui\package.json", "$fixtureRoot\scripts\measure-idle.Tests.ps1", "$fixtureRoot\.github\workflows\ci.yml"
    Remove-Item -LiteralPath "$fixtureRoot\app\ui"
    Remove-Item -LiteralPath "$fixtureRoot\app"

    git -C "$fixtureRoot" add -- 'scripts/measure-idle.ps1'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Messskript konnte nicht gestaged werden.' }
    Remove-Item -LiteralPath "$fixtureRoot\scripts\measure-idle.ps1"
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage '0.1' -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Ein gelöschtes Messskript im Index muss den Modus 0.1 sperren.'
    git -C "$fixtureRoot" -c user.name='QMS Test' -c user.email='test@example.invalid' commit --quiet -m 'Test: Messskript begonnen'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Messhistorie konnte nicht angelegt werden.' }
    git -C "$fixtureRoot" add --update
    git -C "$fixtureRoot" -c user.name='QMS Test' -c user.email='test@example.invalid' commit --quiet -m 'Test: Messskript entfernt'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Messlöschung konnte nicht aufgezeichnet werden.' }
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage '0.1' -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Auch ein nur noch historisches Messskript muss den Modus 0.1 sperren.'

    git -C "$fixtureRoot" add -- 'Cargo.toml'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Manifest konnte nicht gestaged werden.' }
    Remove-Item -LiteralPath "$fixtureRoot\Cargo.toml"
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Ein gelöschtes Manifest im Index darf keine Vorbereitung vortäuschen.'
    git -C "$fixtureRoot" -c user.name='QMS Test' -c user.email='test@example.invalid' commit --quiet -m 'Test: Workspace begonnen'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Historie konnte nicht angelegt werden.' }
    git -C "$fixtureRoot" add --update
    git -C "$fixtureRoot" -c user.name='QMS Test' -c user.email='test@example.invalid' commit --quiet -m 'Test: Manifest entfernt'
    if ($LASTEXITCODE -ne 0) { throw 'Test-Löschung konnte nicht aufgezeichnet werden.' }
    $plan = & "$fixtureRoot\scripts\check.ps1" -Stage V5 -PlanOnly
    Assert-Condition (-not $plan.Allowed) 'Auch ein bereits committetes Löschen darf die Vorbereitung nicht wieder freigeben.'

    # Keep ignore-rule checks after the index fixtures so their staging remains independent.
    Copy-Item -LiteralPath "$projectRoot\.gitignore" -Destination "$fixtureRoot\.gitignore"
    foreach ($sourcePath in @('crates/models/Cargo.toml', 'crates/models/src/lib.rs')) {
        git -C "$fixtureRoot" check-ignore --no-index --quiet -- "$sourcePath"
        Assert-Condition ($LASTEXITCODE -eq 1) "Crate-Quellen dürfen nicht ignoriert werden: $sourcePath"
    }
    foreach ($privatePath in @(
        'models/parakeet/model.onnx',
        'models/parakeet/config.json',
        'crates/models/model.onnx',
        'crates/models/model.gguf',
        'crates/models/model.bin',
        'testdata/private/aufnahme.txt',
        '.env',
        'secret.key'
    )) {
        git -C "$fixtureRoot" check-ignore --no-index --quiet -- "$privatePath"
        Assert-Condition ($LASTEXITCODE -eq 0) "Private Dateien und Modelle müssen ignoriert bleiben: $privatePath"
    }

    Write-Host 'Prüfregeln bestanden: V5, Schritt 0.1, Vollprüfung, Ignore-Regeln sowie Schutz gegen fehlende, begonnene und gelöschte Voraussetzungen.'
} finally {
    if ($createdFixture) {
        $resolvedFixture = (Resolve-Path -LiteralPath "$fixtureRoot").ProviderPath
        $fixture = Get-Item -LiteralPath "$resolvedFixture" -Force
        if ([System.IO.Path]::GetDirectoryName("$resolvedFixture") -ine $projectRoot -or
            $fixture.Name -cne $fixtureName -or ($fixture.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw 'Der temporäre Prüfpfad liegt nicht regulär direkt unter der Projektwurzel.'
        }
        Remove-Item -LiteralPath "$resolvedFixture" -Recurse -Force
    }
}
