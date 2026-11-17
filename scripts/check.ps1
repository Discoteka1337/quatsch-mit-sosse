[CmdletBinding()]
param(
    [ValidateSet('V5', '0.1', 'Full')]
    [string]$Stage = 'Full',
    [switch]$PlanOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$rank = @{ 'V5' = 0; '0.1' = 1; 'Full' = 2 }[$Stage]
$problems = [System.Collections.Generic.List[string]]::new()

$gitRoot = & git -C "$projectRoot" rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0 -or [System.IO.Path]::GetFullPath("$gitRoot") -ine $projectRoot) {
    throw 'Die Projektwurzel muss ein eigenes Git-Repository sein.'
}
$indexOutput = & git -C "$projectRoot" -c core.quotepath=false ls-files -z
if ($LASTEXITCODE -ne 0) { throw 'Der Git-Index konnte nicht geprüft werden.' }
$trackedFiles = @(($indexOutput -join "`n") -split "`0" | Where-Object { $_ -ne '' })
foreach ($file in $trackedFiles) {
    if ($file -match '^testdata/private/|\.(onnx|gguf|bin)$' -or
        ($file -match '\.wav$' -and $file -notmatch '^testdata/public/[^/]+\.wav$')) {
        $problems.Add("Verbotene Datei im Git-Index: $file")
    }
}

# A reduced stage must never hide an already started later stage.
$laterPaths = @()
if ($rank -lt 2) { $laterPaths += @('scripts/privacy-guard.ps1', '.github/workflows/ci.yml') }
if ($rank -eq 0) { $laterPaths += @('Cargo.toml', 'Cargo.lock', 'app', 'crates') }
if ($laterPaths.Count -gt 0) {
    $present = @($laterPaths | Where-Object { Test-Path -LiteralPath "$projectRoot/$_" })
    $indexed = @(& git -C "$projectRoot" ls-files -- $laterPaths)
    if ($LASTEXITCODE -ne 0) { throw 'Der Entwicklungsstand im Git-Index konnte nicht geprüft werden.' }
    $head = & git -C "$projectRoot" rev-parse --verify --quiet 'HEAD'
    $history = @()
    if ($LASTEXITCODE -eq 0) {
        $history = @(& git -C "$projectRoot" log -1 --format='%H' -- $laterPaths)
        if ($LASTEXITCODE -ne 0) { throw 'Die Git-Historie konnte nicht geprüft werden.' }
    }
    if ($present.Count -gt 0 -or $indexed.Count -gt 0 -or $history.Count -gt 0) {
        $problems.Add("Prüfmodus $Stage ist gesperrt: Ein späterer Bauschritt ist bereits begonnen oder in Git aufgezeichnet.")
    }
}

$definitions = @(
    @{ Id = 'rules'; Name = 'Regressionstest der Prüfregeln'; Since = 0; Required = 'scripts/check.Tests.ps1' }
    @{ Id = 'path'; Name = 'Pfad-Regressionstest'; Since = 0; Required = 'scripts/path-check.Tests.ps1' }
    @{ Id = 'git'; Name = 'Git-Whitespace-Prüfung'; Since = 0; Required = '' }
    @{ Id = 'fmt'; Name = 'cargo fmt --all --check'; Since = 1; Required = 'Cargo.toml' }
    @{ Id = 'clippy'; Name = 'cargo clippy --workspace --all-targets -- -D warnings'; Since = 1; Required = 'Cargo.toml' }
    @{ Id = 'cargo-test'; Name = 'cargo test --workspace'; Since = 1; Required = 'Cargo.toml' }
    @{ Id = 'ui-check'; Name = 'npm run check'; Since = 1; Required = 'app/ui/package.json' }
    @{ Id = 'ui-test'; Name = 'npm test'; Since = 1; Required = 'app/ui/package.json' }
    @{ Id = 'privacy'; Name = 'Datenschutz-Wächter'; Since = 2; Required = 'scripts/privacy-guard.ps1' }
)
$checks = foreach ($definition in $definitions) {
    $state = 'Ready'
    $reason = ''
    if ($rank -lt $definition.Since) {
        $state = 'NotScheduled'
        $reason = if ($definition.Since -eq 1) { 'erst ab Schritt 0.1 vorgesehen' } else { 'erst ab Schritt 0.2 vorgesehen' }
    } elseif ($definition.Required -and -not (Test-Path -LiteralPath "$projectRoot/$($definition.Required)" -PathType Leaf)) {
        $state = 'Missing'
        $reason = "Voraussetzung fehlt: $($definition.Required)"
    }
    [pscustomobject]@{ Id = $definition.Id; Name = $definition.Name; State = $state; Reason = $reason }
}
$plan = [pscustomobject]@{
    Stage = $Stage
    Allowed = $problems.Count -eq 0
    CanPass = $problems.Count -eq 0 -and @($checks | Where-Object { $_.State -eq 'Missing' }).Count -eq 0
    Problems = $problems.ToArray()
    Checks = $checks
}
if ($PlanOnly) { return $plan }
if (-not $plan.Allowed) {
    $plan.Problems | ForEach-Object { Write-Host "FEHLER: $_" -ForegroundColor Red }
    exit 1
}

$exitCode = 0
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location -LiteralPath "$projectRoot"
try {
    $env:CARGO_TARGET_DIR = 'C:\qms-target'
    foreach ($check in $checks) {
        if ($check.State -eq 'NotScheduled') {
            Write-Host "NOCH NICHT VORGESEHEN: $($check.Name) - $($check.Reason)"
            continue
        }
        if ($check.State -eq 'Missing') {
            Write-Host "FEHLER: $($check.Name) - $($check.Reason)" -ForegroundColor Red
            $exitCode = 1
            continue
        }
        try {
            switch ($check.Id) {
                'rules' { & "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$projectRoot\scripts\check.Tests.ps1" }
                'path' { & "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$projectRoot\scripts\path-check.Tests.ps1" }
                'git' {
                    git diff --check
                    if ($LASTEXITCODE -ne 0) { throw 'Arbeitskopie enthält Whitespace-Fehler.' }
                    git diff --cached --check
                }
                'fmt' { cargo fmt --all --check }
                'clippy' { cargo clippy --workspace --all-targets -- -D warnings }
                'cargo-test' { cargo test --workspace }
                { $_ -in @('ui-check', 'ui-test') } {
                    Push-Location -LiteralPath "$projectRoot\app\ui"
                    try {
                        if ($check.Id -eq 'ui-check') { npm run check } else { npm test }
                    } finally { Pop-Location }
                }
                'privacy' { & "$PSHOME\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$projectRoot\scripts\privacy-guard.ps1" }
            }
            if ($LASTEXITCODE -ne 0) { throw "Exitcode $LASTEXITCODE" }
            Write-Host "OK: $($check.Name)"
        } catch {
            Write-Host "FEHLER: $($check.Name) - $($_.Exception.Message)" -ForegroundColor Red
            $exitCode = 1
        }
    }
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
if ($exitCode -eq 0) { Write-Host "Alle für $Stage vorgesehenen Prüfungen bestanden." }
exit $exitCode
