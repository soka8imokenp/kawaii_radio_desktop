# Publishes the version in dist/ to radio.kawaii.uz: the builds, the Linux installer script and the signed manifest
# go into the bucket's desk/ (the installed apps update from there; the page links to /download/...).
# Run after release.py and release_linux.sh:
#   powershell -ExecutionPolicy Bypass -File D:\kawaii_radio_exe\native\publish.ps1
$ErrorActionPreference = "Stop"
$dist = Join-Path (Split-Path $PSScriptRoot) "dist"
$cloud = "D:\kawaii_radio_app\cloud"
$j = Get-Content (Join-Path $dist "latest.json") -Raw | ConvertFrom-Json
$names = $j.platforms.PSObject.Properties | ForEach-Object { ($_.Value.url -split '/')[-1] }
Push-Location $cloud
try {
  foreach ($n in $names) {
    Write-Host "desk/$n"
    npx wrangler r2 object put "kawaii-radio/desk/$n" --file (Join-Path $dist $n) --remote
    if ($LASTEXITCODE -ne 0) { throw "upload failed: $n" }
  }
  npx wrangler r2 object put "kawaii-radio/desk/install-linux.sh" --file (Join-Path $dist "install-linux.sh") --content-type "text/x-shellscript" --remote
  if ($LASTEXITCODE -ne 0) { throw "upload failed: install-linux.sh" }
  # the manifest last: the apps see the new version only once its files are there
  npx wrangler r2 object put "kawaii-radio/desk/latest.json" --file (Join-Path $dist "latest.json") --content-type "application/json" --remote
  if ($LASTEXITCODE -ne 0) { throw "upload failed: latest.json" }
  Write-Host "TAYYOR: v$($j.version) radio.kawaii.uz/download/... da"
} finally { Pop-Location }
