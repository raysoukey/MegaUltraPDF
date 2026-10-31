#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Banc de lecture de l'audit de compatibilité : pour chaque PDF de fixtures/corpus/, compare ce que
# MegaUltraPDF (binaire `mega`) lit avec poppler, MuPDF et pdf.js, et écrit target/audit/lecture.csv.
# Colonnes : famille, fichier, octets, mot de passe, statut mega (OK / REFUSÉ / PANIQUE), pages mega /
# poppler / mupdf / pdfjs, similarité du texte de la page 1 (Jaccard %) avec poppler / mupdf / pdfjs,
# écart de rendu de la page 1 (% de pixels) avec poppler / mupdf / pdfjs, `qpdf --check` (code, WARNING),
# erreur mega.
# Outils (surcharges par variables d'environnement) : MEGA_BIN, MEGA_QPDF, MEGA_MUTOOL, MEGA_POPPLER_BIN,
# MEGA_PDFJS_CHECK. Usage : bash scripts/audit/read-bench.sh [dossier ou fichier…]
set -u
root="$(cd "$(dirname "$0")/../.." && pwd)"
MEGA_BIN="${MEGA_BIN:-$root/target/debug/mega.exe}"
MEGA_QPDF="${MEGA_QPDF:-C:\\Program Files\\qpdf 12.4.2\\bin\\qpdf.exe}"
MEGA_MUTOOL="${MEGA_MUTOOL:-$root/tools/dl/mupdf/mutool.exe}"
MEGA_POPPLER_BIN="${MEGA_POPPLER_BIN:-$LOCALAPPDATA/Microsoft/WinGet/Packages/oschwartz10612.Poppler_Microsoft.Winget.Source_8wekyb3d8bbwe/poppler-25.07.0/Library/bin}"
PDFJS="$root/scripts/audit/pdfjs-check.mjs"
out="$root/target/audit"; work="$out/lecture"; mkdir -p "$work"
csv="$out/lecture.csv"
# Mots de passe essayés sur un fichier chiffré (ceux des suites de test publiques et des fixtures du projet).
candidates=(u o user owner view master attachment wwwww secret test password)

targets=("$@"); [ ${#targets[@]} -eq 0 ] && targets=("$root/fixtures/corpus")
mapfile -t files < <(find "${targets[@]}" -type f -iname '*.pdf' | sort)

echo 'famille,fichier,octets,mot_de_passe,mega,pages_mega,pages_poppler,pages_mupdf,pages_pdfjs,texte_poppler,texte_mupdf,texte_pdfjs,rendu_poppler,rendu_mupdf,rendu_pdfjs,qpdf,erreur_mega' > "$csv"
for f in "${files[@]}"; do
  family="$(basename "$(dirname "$f")")"; name="$(basename "$f")"; size="$(stat -c %s "$f")"
  id="$family-${name%.pdf}"; w="$work/$id"; mkdir -p "$w"
  # 1. qpdf : contrôle et mot de passe.
  password=""; qpdf_status="absent"
  if [ -x "$MEGA_QPDF" ] || command -v "$MEGA_QPDF" >/dev/null 2>&1; then
    if "$MEGA_QPDF" --check "$f" 2>&1 | grep -q 'invalid password'; then
      for c in "${candidates[@]}"; do
        if ! "$MEGA_QPDF" --password="$c" --check "$f" 2>&1 | grep -q 'invalid password'; then password="$c"; break; fi
      done
    fi
    rep="$("$MEGA_QPDF" ${password:+--password="$password"} --check "$f" 2>&1)"; code=$?
    qpdf_status="$code/$(printf '%s' "$rep" | grep -c WARNING)"
  fi
  pw=(); [ -n "$password" ] && pw=(--password "$password")
  # 2. mega.
  info="$("$MEGA_BIN" info "$f" "${pw[@]}" 2>&1)"; mrc=$?
  if [ $mrc -eq 101 ]; then mega=PANIQUE; elif [ $mrc -ne 0 ]; then mega=REFUSÉ; else mega=OK; fi
  err=""; [ $mrc -ne 0 ] && err="$(printf '%s' "$info" | tail -1 | tr ',' ';' | cut -c1-160)"
  pages_mega="$(printf '%s' "$info" | sed -n 's/^Pages : //p')"
  if [ "$mega" = OK ]; then
    "$MEGA_BIN" export-text "$f" "${pw[@]}" --pages 1 --no-separators -o "$w/mega.txt" --force >/dev/null 2>&1 || : > "$w/mega.txt"
    "$MEGA_BIN" render "$f" "${pw[@]}" --page 1 --out "$w/mega.png" --force >/dev/null 2>&1 || rm -f "$w/mega.png"
  fi
  # 3. poppler.
  pages_pop=""; sim_pop=""; diff_pop=""
  if [ -x "$MEGA_POPPLER_BIN/pdfinfo.exe" ]; then
    pages_pop="$("$MEGA_POPPLER_BIN/pdfinfo.exe" ${password:+-upw "$password"} "$f" 2>/dev/null | sed -n 's/^Pages: *//p')"
    "$MEGA_POPPLER_BIN/pdftotext.exe" ${password:+-upw "$password"} -f 1 -l 1 "$f" "$w/poppler.txt" >/dev/null 2>&1
    "$MEGA_POPPLER_BIN/pdftoppm.exe" ${password:+-upw "$password"} -f 1 -l 1 -r 72 -png "$f" "$w/poppler" >/dev/null 2>&1
    p1="$(ls "$w"/poppler-*.png 2>/dev/null | head -1)"; [ -n "$p1" ] && mv "$p1" "$w/poppler.png"
  fi
  # 4. MuPDF.
  pages_mu=""; sim_mu=""; diff_mu=""
  if [ -x "$MEGA_MUTOOL" ]; then
    pages_mu="$("$MEGA_MUTOOL" info ${password:+-p "$password"} "$f" 2>/dev/null | sed -n 's/^Pages: *//p' | head -1)"
    "$MEGA_MUTOOL" draw ${password:+-p "$password"} -F txt -o "$w/mupdf.txt" "$f" 1 >/dev/null 2>&1
    "$MEGA_MUTOOL" draw ${password:+-p "$password"} -r 72 -o "$w/mupdf.png" "$f" 1 >/dev/null 2>&1
  fi
  # 5. pdf.js.
  js="$(node "$PDFJS" ${password:+--password "$password"} --png "$w/pdfjs.png" "$f" 2>/dev/null | tail -1)"
  pages_js="$(printf '%s' "$js" | python -I -c 'import json,sys; d=json.loads(sys.stdin.read() or "{}"); print(d.get("pages") if d.get("ok") else "")' 2>/dev/null)"
  printf '%s' "$js" | python -I -c 'import json,sys; d=json.loads(sys.stdin.read() or "{}"); print(d.get("text",""))' > "$w/pdfjs.txt" 2>/dev/null
  printf '%s\n' "$js" > "$w/pdfjs.json"
  # 6. Comparaisons.
  if [ "$mega" = OK ]; then
    [ -s "$w/poppler.txt" ] && sim_pop="$(python -I "$root/scripts/audit/textsim.py" "$w/mega.txt" "$w/poppler.txt")"
    [ -s "$w/mupdf.txt" ] && sim_mu="$(python -I "$root/scripts/audit/textsim.py" "$w/mega.txt" "$w/mupdf.txt")"
    [ -n "$pages_js" ] && sim_js="$(python -I "$root/scripts/audit/textsim.py" "$w/mega.txt" "$w/pdfjs.txt")" || sim_js=""
    for ref in poppler mupdf pdfjs; do
      v=""; [ -f "$w/mega.png" ] && [ -f "$w/$ref.png" ] && v="$(node "$root/scripts/audit/compare-png.mjs" "$w/mega.png" "$w/$ref.png" 2>/dev/null)"
      case $ref in poppler) diff_pop="$v";; mupdf) diff_mu="$v";; pdfjs) diff_js="$v";; esac
    done
  else sim_js=""; diff_js=""; fi
  echo "$family,$name,$size,$password,$mega,$pages_mega,$pages_pop,$pages_mu,$pages_js,$sim_pop,$sim_mu,${sim_js:-},$diff_pop,$diff_mu,${diff_js:-},$qpdf_status,$err" | tee -a "$csv"
done
echo "→ $csv"
