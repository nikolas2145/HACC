$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

# Remove generated benchmark data and IDE files from the Git index only.
# The files remain on disk. Existing Git history is not rewritten.
$trackedData = @(
    '.idea', 'mixed.bin', 'real2g.bin', 'real10g.bin',
    'real2g.hacc', 'real10g_l3.hacc',
    'out_crc', 'out_real2g', 'out_real10g'
)
git rm --cached -r -- $trackedData
if ($LASTEXITCODE -ne 0) { throw 'Could not update the Git index.' }

git add -- .gitignore Cargo.toml Cargo.lock README.md src desktop scripts tests
if ($LASTEXITCODE -ne 0) { throw 'Could not stage release source files.' }
git status --short
