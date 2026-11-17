[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$workflowPath = Join-Path -Path "$projectRoot" -ChildPath '.github/workflows/ci.yml'
if (-not (Test-Path -LiteralPath "$workflowPath" -PathType Leaf)) {
    throw 'Der CI-Workflow fehlt.'
}

$workflow = Get-Content -LiteralPath "$workflowPath" -Encoding UTF8 -Raw
$check = Get-Content -LiteralPath "$PSScriptRoot\check.ps1" -Encoding UTF8 -Raw

function Assert-Match {
    param([string]$Text, [string]$Pattern, [string]$Message)
    if ($Text -notmatch $Pattern) { throw $Message }
}

# Check operational requirements without pinning action releases or display names.
Assert-Match $workflow '(?m)^\s*runs-on:\s*windows-latest\s*$' 'CI muss auf windows-latest laufen.'
foreach ($event in @('push', 'pull_request', 'workflow_dispatch')) {
    Assert-Match $workflow "(?m)^  ${event}:\s*$" "Der CI-Auslöser $event fehlt."
}
Assert-Match $workflow '(?m)^  contents:\s*read\s*$' 'CI benötigt nur lesenden Repositoryzugriff.'
Assert-Match $workflow '(?m)^\s*shell:\s*powershell\s*$' 'CI muss dieselbe Windows-PowerShell wie die lokalen Prüfscripte verwenden.'
Assert-Match $workflow '(?m)^\s*CARGO_TARGET_DIR:\s*''C:\\qms-target''\s*$' 'CI muss das vorgegebene Cargo-Ausgabeverzeichnis nutzen.'
Assert-Match $workflow '(?m)^\s*uses:\s*actions/setup-node@\S+\s*$' 'Node muss in CI eingerichtet werden.'
Assert-Match $workflow '(?m)^\s*cache:\s*''?npm''?\s*$' 'Der npm-Cache fehlt.'
Assert-Match $workflow '(?m)^\s*cache-dependency-path:\s*[''"]app/ui/package-lock\.json[''"]\s*$' 'Der npm-Cache muss das UI-Lockfile verwenden.'
Assert-Match $workflow '(?m)^\s*uses:\s*Swatinem/rust-cache@\S+\s*$' 'Der Cargo-Cache fehlt.'
Assert-Match $workflow '(?m)^\s*cache-directories:\s*''C:\\qms-target''\s*$' 'Der Cargo-Cache muss die tatsächlichen Build-Artefakte erfassen.'
Assert-Match $workflow '(?m)^\s*working-directory:\s*[''"]app/ui[''"]\s*$' 'npm ci muss im UI-Verzeichnis laufen.'
Assert-Match $workflow '(?m)^\s*run:\s*npm ci(?:\s+[^\r\n]+)?\s*$' 'Die reproduzierbare npm-Installation fehlt.'
Assert-Match $workflow '(?m)^\s*run:\s*powershell -NoProfile -ExecutionPolicy Bypass -File "scripts/check\.ps1"\s*$' 'CI muss die vollständige zentrale Prüfung ohne reduzierten Stage-Modus ausführen.'
if ($workflow -match '(?m)^\s*continue-on-error:\s*true\s*$') {
    throw 'Pflichtprüfungen dürfen Fehler nicht ignorieren.'
}

# Verify that delegation to the shared runner still includes the requested checks.
foreach ($command in @(
    'cargo fmt --all --check',
    'cargo clippy --workspace --all-targets -- -D warnings',
    'cargo test --workspace',
    'npm run check',
    'npm test',
    'privacy-guard.ps1',
    'privacy-guard.Tests.ps1'
)) {
    Assert-Match $check ([regex]::Escape($command)) "Die zentrale CI-Prüfung muss $command enthalten."
}

Write-Host 'CI-Vertrag bestanden: Windows, Auslöser, Cargo-/npm-Caches, npm ci und vollständiger gemeinsamer Prüflauf.'
