# Hook Stop : verifie le travail en cours avant que Claude ne rende la main.
# - Fichiers Rust modifies    -> cargo clippy (-D warnings) + cargo test (crates touches)
# - Fichiers app/src modifies -> pnpm check + pnpm test
# Echec -> code 2 et resume sur stderr : Claude doit corriger avant de s'arreter.
# Garde anti-boucle : si stop_hook_active est vrai, on ne bloque pas une seconde fois.
# (Fichier volontairement en ASCII : PowerShell 5.1 lit les .ps1 sans BOM en ANSI.)
$ErrorActionPreference = 'Continue'
$raw = [Console]::In.ReadToEnd()
$active = $false
if ($raw) { try { $active = [bool](($raw | ConvertFrom-Json).stop_hook_active) } catch {} }

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;" + $env:Path }

$changed = @(git status --porcelain 2>$null | ForEach-Object { ($_ -replace '^...', '') -replace '\\', '/' })
if ($changed.Count -eq 0) { exit 0 }

# Securite : si un fichier qui pilote la compilation ou les scripts a change (build.rs,
# Cargo.toml, config cargo, package.json, configs Vite/Svelte, .npmrc), le hook n'execute
# rien automatiquement : il demande une verification manuelle (/verif) apres relecture.
$buildConfig = @($changed | Where-Object {
  $_ -match '(^|/)build\.rs$|(^|/)Cargo\.toml$|^\.cargo/|^rust-toolchain\.toml$|(^|/)package\.json$|^app/(vite|svelte|vitest)\.config\.|^app/\.npmrc$|^app/pnpm-workspace\.yaml$|^\.claude/'
})
if ($buildConfig.Count -gt 0) {
  # Un simple texte sur stdout n'atteint pas Claude : on bloque une fois avec une raison
  # (JSON reconnu par Claude Code), puis on laisse passer si le hook est deja actif.
  if ($active) { exit 0 }
  $reason = "Fichiers de configuration modifies (" + ($buildConfig -join ', ') + ") : le hook Stop n'execute rien automatiquement. Relire ces fichiers, puis lancer /verif avant de conclure."
  $json = @{ decision = 'block'; reason = $reason } | ConvertTo-Json -Compress
  Write-Output $json
  exit 0
}

$rust = @($changed | Where-Object { $_ -match '\.rs$' })
$front = @($changed | Where-Object { $_ -match '^app/src/' })
if ($rust.Count -eq 0 -and $front.Count -eq 0) { exit 0 }

$script:failures = @()
function Invoke-Check($label, $exe, $argList) {
  $out = & $exe @argList 2>&1
  if ($LASTEXITCODE -ne 0) {
    $tail = ($out | Select-Object -Last 25) -join "`n"
    $script:failures += "[KO] $label`n$tail"
  } else {
    Write-Output "[OK] $label"
  }
}

if ($rust.Count -gt 0) {
  $crates = @()
  foreach ($f in $rust) {
    if ($f -match '^crates/mega-core/') { $crates += 'mega-core' }
    elseif ($f -match '^crates/mega-cli/') { $crates += 'mega-cli' }
    elseif ($f -match '^app/src-tauri/') { $crates += 'mega-app' }
    elseif ($f -match '^xtask/') { $crates += 'xtask' }
  }
  $crates = @($crates | Sort-Object -Unique)
  if ($crates.Count -eq 0) { $crates = @('mega-core') }
  Invoke-Check 'cargo clippy --workspace --all-targets -- -D warnings' 'cargo' @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
  foreach ($c in $crates) { Invoke-Check "cargo test -p $c" 'cargo' @('test', '-p', $c) }
}
if ($front.Count -gt 0) {
  Invoke-Check 'pnpm --dir app check' 'pnpm' @('--dir', 'app', 'check')
  Invoke-Check 'pnpm --dir app test' 'pnpm' @('--dir', 'app', 'test')
}

if ($script:failures.Count -gt 0) {
  [Console]::Error.WriteLine("Verifications en echec (hook Stop) :`n" + ($script:failures -join "`n`n"))
  if ($active) { exit 0 }
  exit 2
}
Write-Output '[OK] verifications terminees'
exit 0
