function Import-MsvcEnvironment {
    [CmdletBinding()]
    param()

    $ErrorActionPreference = 'Stop'
    Initialize-CargoOutputDirectories
    $env:VSCMD_SKIP_SENDTELEMETRY = '1'
    $vswherePath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath "$vswherePath" -PathType Leaf)) {
        throw 'vswhere fehlt. Visual Studio 2022 oder 2026 mit Desktop-C++ wird benötigt.'
    }
    $installationPath = & "$vswherePath" -latest -products '*' -version '[17.0,19.0)' -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $installationPath) {
        throw 'Visual Studio 2022 oder 2026 mit Desktop-C++ wurde nicht gefunden.'
    }

    # Avoid adding duplicate search paths when an already prepared shell invokes another script.
    if ($env:VSINSTALLDIR -and $env:VCToolsInstallDir -and
        $env:VSINSTALLDIR.TrimEnd('\') -ieq $installationPath.TrimEnd('\') -and
        $env:VisualStudioVersion -in @('17.0', '18.0') -and
        $env:VSCMD_ARG_TGT_ARCH -eq 'x64' -and $env:VSCMD_ARG_HOST_ARCH -eq 'x64') {
        $runtimeDirectory = Join-Path -Path "$env:VCToolsInstallDir" -ChildPath 'lib\x64'
        $expectedLinker = Join-Path -Path "$env:VCToolsInstallDir" -ChildPath 'bin\Hostx64\x64\link.exe'
        $linker = Get-Command -Name 'link.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ((Test-Path -LiteralPath "$runtimeDirectory\msvcrt.lib" -PathType Leaf) -and
            $runtimeDirectory -in @($env:LIB -split ';' | ForEach-Object { $_.TrimEnd('\') }) -and
            $linker -and $linker.Source -ieq $expectedLinker) {
            return
        }
    }

    $developerScript = Join-Path -Path "$installationPath" -ChildPath 'Common7\Tools\Launch-VsDevShell.ps1'
    if (-not (Test-Path -LiteralPath "$developerScript" -PathType Leaf)) {
        throw 'Die PowerShell-Entwicklungsumgebung von Visual Studio fehlt.'
    }
    # The developer shell's command-line environment import can corrupt Unicode values.
    $cargoTargetDirectory = $env:CARGO_TARGET_DIR
    try {
        & "$developerScript" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    } finally {
        $env:CARGO_TARGET_DIR = $cargoTargetDirectory
    }
    # The developer shell consumes this flag; keep it set for nested tool invocations.
    $env:VSCMD_SKIP_SENDTELEMETRY = '1'
    if (-not $env:VCToolsInstallDir -or
        -not (Test-Path -LiteralPath "$env:VCToolsInstallDir\lib\x64\msvcrt.lib" -PathType Leaf)) {
        throw 'Die MSVC-Umgebung wurde nicht vollständig geladen; msvcrt.lib fehlt.'
    }
    Write-Host "MSVC-Umgebung: $installationPath (x64)"
}

function Initialize-CargoOutputDirectories {
    [CmdletBinding()]
    param()

    if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $env:CARGO_TARGET_DIR = 'C:\qms-target'
    }
    # sherpa-rs-sys 0.6.8 copies DLLs before newer Cargo creates these directories.
    foreach ($profile in @('debug', 'release')) {
        foreach ($directory in @('deps', 'examples')) {
            $outputPath = Join-Path -Path "$env:CARGO_TARGET_DIR" -ChildPath "$profile\$directory"
            [System.IO.Directory]::CreateDirectory("$outputPath") | Out-Null
        }
    }
}
