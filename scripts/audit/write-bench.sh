#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Banc d'écriture de l'audit de compatibilité sur le corpus réel : pour chaque PDF de fixtures/corpus/ que
# `mega` ouvre, applique quatre éditions par le CLI (rotation d'une page, filigrane Unicode, métadonnées,
# optimisation) et juge chaque sortie avec qpdf, MuPDF, pdf.js et poppler. Écrit target/audit/ecriture.csv.
# Colonnes : famille, fichier, édition, statut mega, octets avant / après, `qpdf --check` (code/WARNING),
# avertissements MuPDF, pdf.js (pages ou erreur), part des mots de la page 1 d'avant retrouvés après (poppler, %),
# clés du catalogue perdues, signets avant/après, pièces jointes avant/après, AcroForm avant/après, erreur.
# Usage : bash scripts/audit/write-bench.sh [dossier ou fichier…]   (mêmes variables que read-bench.sh)
set -u
root="$(cd "$(dirname "$0")/../.." && pwd)"
MEGA_BIN="${MEGA_BIN:-$root/target/debug/mega.exe}"
MEGA_QPDF="${MEGA_QPDF:-C:\\Program Files\\qpdf 12.4.2\\bin\\qpdf.exe}"
MEGA_MUTOOL="${MEGA_MUTOOL:-$root/tools/dl/mupdf/mutool.exe}"
MEGA_POPPLER_BIN="${MEGA_POPPLER_BIN:-$LOCALAPPDATA/Microsoft/WinGet/Packages/oschwartz10612.Poppler_Microsoft.Winget.Source_8wekyb3d8bbwe/poppler-25.07.0/Library/bin}"
PDFJS="$root/scripts/audit/pdfjs-check.mjs"
out="$root/target/audit"; work="$out/ecriture"; mkdir -p "$work"
csv="$out/ecriture.csv"
candidates=(u o user owner view master attachment wwwww secret test password)

targets=("$@"); [ ${#targets[@]} -eq 0 ] && targets=("$root/fixtures/corpus")
mapfile -t files < <(find "${targets[@]}" -type f -iname '*.pdf' | sort)

catalog_keys() { # clés de premier niveau du catalogue, triées, séparées par des espaces
  "$MEGA_MUTOOL" show ${2:+-p "$2"} -b "$1" trailer/Root 2>/dev/null | python -I -c '
import sys,re
s=sys.stdin.read(); depth=0; keys=[]
for m in re.finditer(r"<<|>>|/([A-Za-z0-9_.-]+)", s):
    t=m.group(0)
    if t=="<<": depth+=1
    elif t==">>": depth-=1
    elif depth==1 and m.group(1): keys.append(m.group(1))
# dans un dictionnaire, une clé sur deux est un nom de clé ; on garde les noms suivis d une valeur (approx.)
print(" ".join(sorted(set(k for i,k in enumerate(keys) if i%2==0 or k not in keys[:i]))))'
}
pdfjs_field() { printf '%s' "$1" | python -I -c "import json,sys; d=json.loads(sys.stdin.read() or '{}'); v=d.get('$2'); print('' if v is None else v)" 2>/dev/null; }

echo 'famille,fichier,edition,mega,octets_avant,octets_apres,qpdf,mupdf_avertissements,pdfjs,texte_poppler,cles_catalogue_perdues,signets_avant,signets_apres,pj_avant,pj_apres,acroform_avant,acroform_apres,erreur' > "$csv"
for f in "${files[@]}"; do
  family="$(basename "$(dirname "$f")")"; name="$(basename "$f")"; size="$(stat -c %s "$f")"
  id="$family-${name%.pdf}"; w="$work/$id"; mkdir -p "$w"
  password=""
  if "$MEGA_QPDF" --check "$f" 2>&1 | grep -q 'invalid password'; then
    for c in "${candidates[@]}"; do
      if ! "$MEGA_QPDF" --password="$c" --check "$f" 2>&1 | grep -q 'invalid password'; then password="$c"; break; fi
    done
  fi
  pw=(); [ -n "$password" ] && pw=(--password "$password")
  "$MEGA_BIN" info "$f" "${pw[@]}" >/dev/null 2>&1 || continue
  "$MEGA_POPPLER_BIN/pdftotext.exe" ${password:+-upw "$password"} -f 1 -l 1 "$f" "$w/avant.txt" >/dev/null 2>&1
  before_js="$(node "$PDFJS" ${password:+--password "$password"} "$f" 2>/dev/null | tail -1)"
  keys_before="$(catalog_keys "$f" "$password")"
  ol_b="$(pdfjs_field "$before_js" outlineCount)"; at_b="$(pdfjs_field "$before_js" attachmentCount)"; af_b="$(pdfjs_field "$before_js" isAcroForm)"
  for edit in rotation filigrane metadonnees optimisation; do
    o="$w/$edit.pdf"; rm -f "$o"
    case $edit in
      rotation) args=(rotate "$f" --pages 1 --degrees 90 -o "$o");;
      filigrane) args=(watermark "$f" --text "AUDIT — été 2026 Ω" --size 40 -o "$o");;
      metadonnees) args=(meta "$f" --title "Audit — été" --author "Été Ω" -o "$o");;
      optimisation) args=(optimize "$f" -o "$o");;
    esac
    res="$("$MEGA_BIN" "${args[@]}" "${pw[@]}" 2>&1)"; rc=$?
    if [ $rc -eq 101 ]; then mega=PANIQUE; elif [ $rc -ne 0 ]; then mega=REFUSÉ; else mega=OK; fi
    err=""; [ $rc -ne 0 ] && err="$(printf '%s' "$res" | tail -1 | tr ',' ';' | cut -c1-160)"
    if [ "$mega" != OK ] || [ ! -f "$o" ]; then
      echo "$family,$name,$edit,$mega,$size,,,,,,,,,,,,,$err" | tee -a "$csv"; continue
    fi
    size_after="$(stat -c %s "$o")"
    rep="$("$MEGA_QPDF" ${password:+--password="$password"} --check "$o" 2>&1)"; code=$?
    qpdf_status="$code/$(printf '%s' "$rep" | grep -c WARNING)"
    mu="$("$MEGA_MUTOOL" draw ${password:+-p "$password"} -o /dev/null "$o" 2>&1 | grep -ciE '^(warning|error)')"
    after_js="$(node "$PDFJS" ${password:+--password "$password"} "$o" 2>/dev/null | tail -1)"
    js_pages="$(pdfjs_field "$after_js" pages)"; js_err="$(pdfjs_field "$after_js" error | tr ',' ';' | cut -c1-80)"
    js="${js_pages:-ERREUR $js_err}"
    "$MEGA_POPPLER_BIN/pdftotext.exe" ${password:+-upw "$password"} -f 1 -l 1 "$o" "$w/$edit.txt" >/dev/null 2>&1
    sim="$(python -I "$root/scripts/audit/textsim.py" --contient "$w/avant.txt" "$w/$edit.txt")"
    keys_after="$(catalog_keys "$o" "$password")"
    lost="$(comm -23 <(tr ' ' '\n' <<<"$keys_before" | sort -u) <(tr ' ' '\n' <<<"$keys_after" | sort -u) | tr '\n' ' ' | sed 's/ $//')"
    ol_a="$(pdfjs_field "$after_js" outlineCount)"; at_a="$(pdfjs_field "$after_js" attachmentCount)"; af_a="$(pdfjs_field "$after_js" isAcroForm)"
    echo "$family,$name,$edit,$mega,$size,$size_after,$qpdf_status,$mu,$js,$sim,$lost,$ol_b,$ol_a,$at_b,$at_a,$af_b,$af_a," | tee -a "$csv"
  done
done
echo "→ $csv"
