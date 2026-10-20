$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$oldHead = (git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Could not read the local commit.' }
$oldTag = (git rev-parse refs/tags/v1.0.0).Trim()
if ($LASTEXITCODE -ne 0) { throw 'The local v1.0.0 tag is missing.' }

# Check the remote before changing the only release commit and tag.
$refs = git ls-remote origin refs/heads/master refs/tags/v1.0.0
if ($LASTEXITCODE -ne 0) { throw 'Could not read origin. No local Git history was changed.' }
$remoteHead = $null
$remoteTag = $null
foreach ($line in $refs) {
    $fields = $line -split '\s+'
    if ($fields.Count -lt 2) { continue }
    if ($fields[1] -eq 'refs/heads/master') { $remoteHead = $fields[0] }
    if ($fields[1] -eq 'refs/tags/v1.0.0') { $remoteTag = $fields[0] }
}
if ($remoteHead -and $remoteHead -ne $oldHead) {
    throw "origin/master changed to $remoteHead. Review it before rewriting history."
}
if ($remoteTag -and $remoteTag -ne $oldTag) {
    throw "The remote v1.0.0 tag changed to $remoteTag. Review it before rewriting history."
}

git diff --cached --quiet
if ($LASTEXITCODE -ne 0) { throw 'The Git index already contains staged changes. Review them first.' }
git add -- desktop/HaccGui.cs desktop/HaccSetup.cs scripts/build-release.ps1 `
    scripts/publish-english-release.ps1 src/archive.rs src/bin/hacc.rs src/codec.rs src/format.rs
if ($LASTEXITCODE -ne 0) { throw 'Could not stage the English source files.' }
git commit --amend --no-edit
if ($LASTEXITCODE -ne 0) { throw 'Could not amend the release commit.' }
git tag -f v1.0.0
if ($LASTEXITCODE -ne 0) { throw 'Could not update the release tag.' }

# Exact leases prevent overwriting any remote work added after the check.
$headLease = "--force-with-lease=refs/heads/master:$remoteHead"
git push $headLease origin master
if ($LASTEXITCODE -ne 0) { throw 'Could not push master. The local commit and tag are ready.' }
$tagLease = "--force-with-lease=refs/tags/v1.0.0:$remoteTag"
git push $tagLease origin v1.0.0
if ($LASTEXITCODE -ne 0) { throw 'Could not push the v1.0.0 tag. The branch was updated.' }

git status -sb
git log -1 --oneline
