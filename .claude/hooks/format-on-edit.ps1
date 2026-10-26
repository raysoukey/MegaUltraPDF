# Hook PostToolUse : reformate le fichier que Claude vient d'ecrire ou de modifier.
# Entree : JSON sur stdin (tool_input.file_path). Sortie : rien ; code 0 toujours.
# Securite : seul un fichier situe DANS le depot est traite, et le chemin est passe
# apres `--` pour ne jamais etre interprete comme une option.
# (Fichier volontairement en ASCII : PowerShell 5.1 lit les .ps1 sans BOM en ANSI.)
$ErrorActionPreference = 'SilentlyContinue'
$raw = [Console]::In.ReadToEnd()
if (-not $raw) { exit 0 }
try { $payload = $raw | ConvertFrom-Json } catch { exit 0 }
$file = [string]$payload.tool_input.file_path
if (-not $file -or $file -like '-*' -or -not (Test-Path -LiteralPath $file -PathType Leaf)) { exit 0 }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path.TrimEnd('\')
$full = (Resolve-Path -LiteralPath $file).Path
if (-not $full.StartsWith($root + '\', [System.StringComparison]::OrdinalIgnoreCase)) { exit 0 }
$relative = $full.Substring($root.Length + 1) -replace '\\', '/'
if ($relative -match '(^|/)(target|node_modules|vendor|\.svelte-kit|build)/') { exit 0 }

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;" + $env:Path }

$ext = [System.IO.Path]::GetExtension($full).ToLowerInvariant()
$appDir = Join-Path $root 'app'

if ($ext -eq '.rs') {
  & rustfmt --edition 2024 --quiet -- $full | Out-Null
}
elseif (($relative -match '^app/') -and ($relative -notmatch '^app/src-tauri/') -and ($ext -in '.ts', '.js', '.svelte', '.json', '.css', '.html')) {
  & pnpm --dir $appDir exec prettier --write --log-level silent -- $full | Out-Null
}
exit 0
