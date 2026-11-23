[CmdletBinding()]
param(
    [string]$RepositoryRoot = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$findings = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$documents = [System.Collections.Generic.List[object]]::new()

function Read-Git {
    param([string[]]$Arguments)

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = (Get-Command -Name 'git.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    # Quote native arguments without invoking a shell; preserve NUL-delimited paths and UTF-8.
    $quoted = foreach ($argument in (@('-C', $script:root, '-c', 'core.quotepath=false') + $Arguments)) {
        $escaped = [regex]::Replace($argument, '(\\*)"', '$1$1\"')
        $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
        '"' + $escaped + '"'
    }
    $startInfo.Arguments = $quoted -join ' '
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) { throw 'Git konnte nicht gestartet werden.' }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $null = $stderrTask.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw 'Git konnte die Prüfdateien nicht lesen.' }
        return $stdout
    } finally { $process.Dispose() }
}

function Test-ProhibitedIndexPath {
    param([string]$Path)
    return $Path -match '^testdata/private/|\.(onnx(\.data)?|gguf|bin)$' -or
        $Path -match '(^|/)((turbo-)?tokens\.txt|sherpa-onnx-(nemo-parakeet|whisper)-[^/]*\.tar\.bz2)$' -or
        $Path -match '(^|/)\.env($|/|\.(?!example($|/)))|\.(key|pem)($|/)' -or
        ($Path -match '\.wav$' -and $Path -notmatch '^testdata/public/[^/]+\.wav$')
}

function Test-ScanPath {
    param([string]$Path)
    # Only these two rule/fixture files contain intentional forbidden examples.
    if ($Path -in @('scripts/privacy-guard.ps1', 'scripts/privacy-guard.Tests.ps1')) { return $false }
    if (Test-ProhibitedIndexPath -Path $Path) { return $false }
    return $Path -match '\.(rs|toml|js|mjs|cjs|ts|mts|cts|jsx|tsx|svelte|vue|html|json|ps1|psm1|psd1|cs|fs|vb|c|cc|cpp|h|hpp|py|sh|bat|cmd|yml|yaml)$' -or
        $Path -match '(^|/)Cargo\.lock$'
}

function Add-Finding {
    param([string]$Rule, [string]$Path, [string]$Source, [int]$Line = 0)
    $location = if ($Line -gt 0) { "${Path}:$Line" } else { $Path }
    # Never include source lines, matched values, audio or tokens in diagnostics.
    $null = $script:findings.Add("[$Rule] $location ($Source)")
}

function Get-HttpAliases {
    param([string]$Content)

    # TOML allows the same dependency table as inline, header or dotted keys.
    $key = '(?:"[^"\r\n]+"|''[^''\r\n]+''|[\w-]+)'
    $keyPrefix = '(?:' + $key + '[ \t]*\.[ \t]*)*'
    $aliasKey = '["'']?(?<alias>[\w-]+)["'']?'
    $packageKey = '(?:package|"package"|''package'')'
    $packageValue = '[ \t]*=[ \t]*["''](?:reqwest|ureq|hyper)["'']'
    $declarations = @([regex]::Matches($Content, '(?m)^[ \t]*' + $keyPrefix + $aliasKey + '[ \t]*=[ \t]*\{(?<body>[^}]+)\}'))
    $declarations += @([regex]::Matches($Content, '(?m)^[ \t]*\[' + $keyPrefix + '["'']?(?:dependencies|dev-dependencies|build-dependencies)["'']?[ \t]*\.[ \t]*' + $aliasKey + '[ \t]*\][^\r\n]*(?<body>(?:\r?\n(?![ \t]*\[)[^\r\n]*)*)'))
    foreach ($declaration in $declarations) {
        if ($declaration.Groups['body'].Value -match ('(?m)(?<![\w-])' + $packageKey + $packageValue)) {
            $declaration.Groups['alias'].Value.Replace('-', '_')
        }
    }
    foreach ($declaration in [regex]::Matches($Content, '(?m)^[ \t]*' + $keyPrefix + $aliasKey + '[ \t]*\.[ \t]*' + $packageKey + $packageValue)) {
        $declaration.Groups['alias'].Value.Replace('-', '_')
    }
}

function Test-Content {
    param([string]$Path, [string]$Content, [string]$Source, [string[]]$HttpAliases = @())

    $manifest = $Path -match '(^|/)(Cargo\.(toml|lock)|package(-lock)?\.json|npm-shrinkwrap\.json)$'
    $networkModule = $Path -cin @('crates/models/src/net.rs', 'crates/llm/src/net.rs')
    $telemetryPattern = '(?i)(?<![\w])(?:sentry|posthog|mixpanel)(?![a-z0-9])'
    # "segment" alone is a normal identifier; package/import references are forbidden.
    $segmentPattern = '(?i)@segment/|\bsegment\s*::|\b(?:use|extern\s+crate)\s+segment\b|(?:from\s*|require\s*\(\s*|import\s*\(?\s*)["'']segment(?:[\-/][^"'']*)?["'']'
    if ($manifest) { $segmentPattern += '|(?i)(?<![\w-])segment(?![\w-])|(?i)\banalytics-node\b' }
    $capturePattern = '(?i)\b(?:BitBlt|StretchBlt|PrintWindow|CopyFromScreen)\b|\b(?:Windows\s*(?:::|\.)\s*)?Graphics\s*(?:::|\.)\s*(?:\{[^;]*?\b)?Capture\b|\bGraphics_Capture(?:_[\w]+)?\b'
    $httpNames = @('reqwest', 'ureq', 'hyper') + @($HttpAliases | ForEach-Object { [regex]::Escape($_) })
    $httpPattern = '(?i)(?<![\w])(?:' + ($httpNames -join '|') + ')(?![\w])'

    $rules = @(
        @{ Code = 'PRIVACY_TELEMETRY'; Pattern = $telemetryPattern }
        @{ Code = 'PRIVACY_TELEMETRY'; Pattern = $segmentPattern }
        @{ Code = 'PRIVACY_CAPTURE'; Pattern = $capturePattern }
    )
    if (-not $manifest -and -not $networkModule) {
        $rules += @{ Code = 'PRIVACY_HTTP'; Pattern = $httpPattern }
    }
    foreach ($rule in $rules) {
        foreach ($match in [regex]::Matches($Content, $rule.Pattern)) {
            $line = 1 + [regex]::Matches($Content.Substring(0, $match.Index), "`n").Count
            Add-Finding -Rule $rule.Code -Path $Path -Source $Source -Line $line
        }
    }
}

try {
    if (-not $RepositoryRoot) { $RepositoryRoot = Join-Path -Path "$PSScriptRoot" -ChildPath '..' }
    $script:root = (Resolve-Path -LiteralPath "$RepositoryRoot").ProviderPath.TrimEnd('\', '/')
    $gitRoot = (Read-Git -Arguments @('rev-parse', '--show-toplevel')).TrimEnd("`r", "`n")
    if ([System.IO.Path]::GetFullPath("$gitRoot").TrimEnd('\', '/') -ine $script:root) {
        throw 'Die Projektwurzel muss ein eigenes Git-Repository sein.'
    }

    $index = Read-Git -Arguments @('ls-files', '--stage', '-z')
    $blobCache = @{}
    foreach ($entry in ($index -split "`0")) {
        if (-not $entry) { continue }
        if ($entry -notmatch '\A(?<mode>[0-7]{6}) (?<hash>[a-f0-9]+) (?<stage>[0-3])\t(?<path>[\s\S]+)\z') {
            throw 'Der Git-Index enthält einen nicht lesbaren Eintrag.'
        }
        $path = $Matches.path
        $hash = $Matches.hash
        $mode = $Matches.mode
        $stage = $Matches.stage
        if (Test-ProhibitedIndexPath -Path $path) {
            Add-Finding -Rule 'PRIVACY_INDEX' -Path $path -Source 'Git-Index'
            continue
        }
        if (-not (Test-ScanPath -Path $path)) { continue }
        if ($stage -ne '0' -or $mode -notin @('100644', '100755')) {
            Add-Finding -Rule 'PRIVACY_SCAN' -Path $path -Source 'kein regulärer, konfliktfreier Codeeintrag'
            continue
        }
        if (-not $blobCache.ContainsKey($hash)) {
            $blobCache[$hash] = Read-Git -Arguments @('cat-file', 'blob', $hash)
        }
        $documents.Add([pscustomobject]@{ Path = $path; Content = $blobCache[$hash]; Source = 'Git-Index' })
    }

    $worktree = Read-Git -Arguments @('ls-files', '--cached', '--others', '--exclude-standard', '-z')
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($path in ($worktree -split "`0")) {
        if (-not $path -or -not $seen.Add($path) -or -not (Test-ScanPath -Path $path)) { continue }
        $fullPath = [System.IO.Path]::GetFullPath((Join-Path -Path "$script:root" -ChildPath "$path"))
        if (-not $fullPath.StartsWith($script:root + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw 'Ein Prüfpfad liegt außerhalb der Projektwurzel.'
        }
        # Do not follow links to private files or paths outside the repository.
        $ancestor = $fullPath
        $linked = $false
        while ($ancestor -ine $script:root) {
            if (Test-Path -LiteralPath "$ancestor") {
                $item = Get-Item -LiteralPath "$ancestor" -Force
                if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { $linked = $true; break }
            }
            $ancestor = [System.IO.Path]::GetDirectoryName("$ancestor")
        }
        if ($linked) {
            Add-Finding -Rule 'PRIVACY_SCAN' -Path $path -Source 'verknüpfter Codepfad'
            continue
        }
        if (-not (Test-Path -LiteralPath "$fullPath" -PathType Leaf)) { continue }
        $content = [System.IO.File]::ReadAllText("$fullPath", [System.Text.Encoding]::UTF8)
        $documents.Add([pscustomobject]@{ Path = $path; Content = $content; Source = 'Arbeitskopie' })
    }

    $aliasScopes = foreach ($document in $documents) {
        if ($document.Path -match '(^|/)Cargo\.toml$') {
            $scope = $document.Path.Substring(0, $document.Path.Length - 'Cargo.toml'.Length)
            foreach ($alias in @(Get-HttpAliases -Content $document.Content)) {
                [pscustomobject]@{ Scope = $scope; Alias = $alias; Source = $document.Source }
            }
        }
    }
    foreach ($document in $documents) {
        $aliases = @($aliasScopes | Where-Object {
            $_.Source -eq $document.Source -and $document.Path.StartsWith($_.Scope, [System.StringComparison]::Ordinal)
        } | ForEach-Object { $_.Alias })
        Test-Content -Path $document.Path -Content $document.Content -Source $document.Source -HttpAliases $aliases
    }

    if ($findings.Count -gt 0) {
        $findings | Sort-Object | ForEach-Object { Write-Host "FEHLER: $_" }
        Write-Host "Datenschutz-Wächter fehlgeschlagen: $($findings.Count) Fundstellen."
        exit 1
    }
    Write-Host 'Datenschutz-Wächter bestanden: Git-Index und Arbeitskopie geprüft.'
    exit 0
} catch {
    # Error details can contain file content; fail closed without echoing them.
    Write-Host 'FEHLER: [PRIVACY_SCAN] Datenschutzprüfung nicht vollständig möglich. Repository, Git und Dateizugriff prüfen.'
    exit 1
}
