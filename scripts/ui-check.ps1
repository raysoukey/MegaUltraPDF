# SPDX-License-Identifier: GPL-3.0-or-later
# Lance l'application en mode developpement sur un fichier, deroule un scenario CDP
# (scripts/ui-check.mjs), puis arrete l'application.
# Usage : scripts/ui-check.ps1 -Pdf <fichier.pdf> -Scenario <scenario.json> -Out <dossier>
param(
  [string]$Pdf = "",
  [Parameter(Mandatory = $true)][string]$Scenario,
  [string]$Out = "target/ui-check"
)
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
New-Item -ItemType Directory -Force $Out | Out-Null
$log = Join-Path $Out 'tauri-dev.log'
$arg = if ($Pdf) { " -- -- `"$Pdf`"" } else { "" }
$cmd = "pnpm --dir app tauri dev$arg > `"$log`" 2>&1"
$proc = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $cmd -PassThru -WindowStyle Hidden
$deadline = (Get-Date).AddSeconds(300); $app = $null
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 3
  $app = Get-Process -Name "mega-app" -ErrorAction SilentlyContinue
  if ($app) { break }
}
if (-not $app) { Write-Output "L'application n'a pas demarre :"; Get-Content $log -Tail 20; exit 1 }
Start-Sleep -Seconds 6
node "$PSScriptRoot\ui-check.mjs" $Scenario $Out
$code = $LASTEXITCODE
Get-Process -Name "mega-app" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Get-Process -Name "node", "cargo" -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -gt (Get-Date).AddMinutes(-10) } | Stop-Process -Force -ErrorAction SilentlyContinue
exit $code
