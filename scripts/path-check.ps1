[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$projectRoot = (Resolve-Path -LiteralPath "$PSScriptRoot\..").ProviderPath
$testDirectory = Join-Path -Path "$projectRoot" -ChildPath 'Test Ordner ß'
$previousTarget = $env:CARGO_TARGET_DIR
$createdTestDirectory = $false
$locationPushed = $false
$exitCode = 0

try {
    if (Test-Path -LiteralPath "$testDirectory") {
        throw "Der Testordner existiert bereits und bleibt unverändert: $testDirectory"
    }

    New-Item -Path "$testDirectory" -ItemType Directory | Out-Null
    $createdTestDirectory = $true
    New-Item -Path "$testDirectory\src" -ItemType Directory | Out-Null

    $manifest = @'
[package]
name = "qms-path-check"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]
'@
    $rustSource = @'
fn main() {
    println!("Hello, world!");
}
'@
    $package = @'
{
  "name": "qms-path-check",
  "version": "0.0.0",
  "private": true
}
'@
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    $manifestPath = Join-Path -Path "$testDirectory" -ChildPath 'Cargo.toml'
    [System.IO.File]::WriteAllText("$manifestPath", $manifest, $utf8)
    [System.IO.File]::WriteAllText("$testDirectory\src\main.rs", $rustSource, $utf8)
    [System.IO.File]::WriteAllText("$testDirectory\package.json", $package, $utf8)

    $env:CARGO_TARGET_DIR = 'C:\qms-target'
    Push-Location -LiteralPath "$testDirectory"
    $locationPushed = $true
    Write-Host "Testordner: $testDirectory"
    Write-Host "Rust-Build-Ausgabe: $env:CARGO_TARGET_DIR"

    $previousBuildEnvironment = [Environment]::GetEnvironmentVariables('Process')
    try {
        $vswherePath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
        $installationPath = & "$vswherePath" -latest -products 'Microsoft.VisualStudio.Product.BuildTools' -version '[17.0,18.0)' -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $installationPath) {
            throw 'Visual Studio 2022 Build Tools mit Desktop-C++ wurden nicht gefunden.'
        }
        $developerScript = Join-Path -Path "$installationPath" -ChildPath 'Common7\Tools\Launch-VsDevShell.ps1'
        $env:VSCMD_SKIP_SENDTELEMETRY = '1'
        & "$developerScript" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
        Write-Host "MSVC-Umgebung: $installationPath (x64)"

        $cargoExecutable = (Get-Command -Name 'cargo' -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        & "$cargoExecutable" build --offline --manifest-path "$manifestPath"
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build meldet Exitcode $LASTEXITCODE."
        }
        Write-Host 'Rust-Build: OK'
    } catch {
        $exitCode = 1
        Write-Host "Rust-Build: FEHLER - $($_.Exception.Message)" -ForegroundColor Red
    } finally {
        foreach ($name in [Environment]::GetEnvironmentVariables('Process').Keys) {
            if (-not $previousBuildEnvironment.Contains($name)) {
                [Environment]::SetEnvironmentVariable($name, $null, 'Process')
            }
        }
        foreach ($name in $previousBuildEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previousBuildEnvironment[$name], 'Process')
        }
    }

    try {
        $npmExecutable = (Get-Command -Name 'npm.cmd' -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        & "$npmExecutable" install --offline --ignore-scripts --no-audit --no-fund --update-notifier=false
        if ($LASTEXITCODE -ne 0) {
            throw "npm install meldet Exitcode $LASTEXITCODE."
        }
        Write-Host 'npm-Installation: OK'
    } catch {
        $exitCode = 1
        Write-Host "npm-Installation: FEHLER - $($_.Exception.Message)" -ForegroundColor Red
    }
} catch {
    $exitCode = 1
    Write-Host "Pfadtest: FEHLER - $($_.Exception.Message)" -ForegroundColor Red
} finally {
    if ($locationPushed) {
        Pop-Location
    }
    $env:CARGO_TARGET_DIR = $previousTarget

    if ($createdTestDirectory) {
        try {
            $directory = Get-Item -LiteralPath "$testDirectory" -Force
            $resolvedTestDirectory = (Resolve-Path -LiteralPath "$testDirectory").ProviderPath
            $parentDirectory = [System.IO.Path]::GetDirectoryName("$resolvedTestDirectory")
            if ($parentDirectory -ine $projectRoot -or $directory.Name -cne 'Test Ordner ß' -or
                ($directory.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
                throw 'Der Aufräumpfad ist kein regulärer Testordner direkt unter der Projektwurzel.'
            }
            Remove-Item -LiteralPath "$resolvedTestDirectory" -Recurse -Force
            Write-Host 'Temporärer Testordner entfernt.'
        } catch {
            $exitCode = 1
            Write-Host "Aufräumen: FEHLER - $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}

if ($exitCode -eq 0) {
    Write-Host 'Pfadtest erfolgreich: Rust und npm funktionieren mit Leerzeichen und ß.'
} else {
    Write-Host 'Pfadtest fehlgeschlagen. Die oben genannten Fehler müssen geprüft werden.'
}
exit $exitCode
