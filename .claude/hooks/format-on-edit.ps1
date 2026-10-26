# Hook PostToolUse : reformate le fichier que Claude vient d'ecrire ou de modifier.
# Entree : JSON sur stdin (tool_input.file_path). Sortie : rien ; code 0 toujours.
# (Fichier volontairement en ASCII : PowerShell 5.1 lit les .ps1 sans BOM en ANSI.)
$ErrorActionPreference = 'SilentlyContinue'
$raw = [Console]::In.ReadToEnd()
if (-not $raw) { exit 0 }
try { $payload = $raw | ConvertFrom-Json } catch { exit 0 }
$file = $payload.tool_input.file_path
if (-not $file -or -not (Test-Path -LiteralPath $file)) { exit 0 }

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;" + $env:Path }

$ext = [System.IO.Path]::GetExtension($file).ToLowerInvariant()
$normalized = $file -replace '\\', '/'
$inFrontend = ($normalized -match '/app/') -and ($normalized -notmatch '/src-tauri/')
$appDir = Join-Path $PSScriptRoot '..\..\app'

if ($ext -eq '.rs') {
  & rustfmt --edition 2024 --quiet $file | Out-Null
}
elseif ($inFrontend -and ($ext -in '.ts', '.js', '.svelte', '.json', '.css', '.html')) {
  & pnpm --dir $appDir exec prettier --write --log-level silent $file | Out-Null
}
exit 0
