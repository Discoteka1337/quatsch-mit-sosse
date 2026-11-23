[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$env:CARGO_TARGET_DIR = 'C:\qms-target'

. "$PSScriptRoot\import-msvc.ps1"
Import-MsvcEnvironment

# Exercise serde_json's build script from the workspace lockfile without building the app.
cargo check --frozen --manifest-path "$PSScriptRoot\..\Cargo.toml" --package "serde_json"
exit $LASTEXITCODE
