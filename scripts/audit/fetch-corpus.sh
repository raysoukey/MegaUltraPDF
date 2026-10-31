#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Récupère le corpus public de PDF réels de l'audit de compatibilité
# (docs/audit/2026-10-31-compatibilite-pdf.md) dans fixtures/corpus/<famille>/, dossier ignoré par Git
# (voir fixtures/corpus/MANIFEST.md pour la provenance, les licences et les sommes SHA-256).
# Sources : suite de test pdf.js (Apache-2.0), suite qpdf (Apache-2.0), corpus veraPDF (CC BY 4.0).
# Usage : bash scripts/audit/fetch-corpus.sh [--force]    # imprime la somme SHA-256 de chaque fichier
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)/fixtures/corpus"
force=0; [ "${1:-}" = "--force" ] && force=1
pdfjs='https://raw.githubusercontent.com/mozilla/pdf.js/master/test/pdfs/'
qpdf='https://raw.githubusercontent.com/qpdf/qpdf/main/qpdf/qtest/qpdf/'
vera='https://raw.githubusercontent.com/veraPDF/veraPDF-corpus/master/'

# Encode un chemin distant pour une URL (espaces, virgules, égal).
urlencode() { python -I -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$1"; }

# famille|base|nom distant|nom local (vide = nom distant)
entries='
producteurs|pdfjs|tracemonkey.pdf|
producteurs|pdfjs|TAMReview.pdf|
producteurs|pdfjs|openoffice.pdf|
producteurs|pdfjs|bug1937438_from_word.pdf|
producteurs|pdfjs|bug1937438_af_from_latex.pdf|
producteurs|pdfjs|perf_text_office_nonembedded.pdf|
producteurs|pdfjs|mixedfonts.pdf|
producteurs|pdfjs|franz.pdf|
producteurs|qpdf|form-empty-from-odt.pdf|
producteurs|qpdf|form-filled-by-acrobat.pdf|
formulaires|pdfjs|160F-2019.pdf|
formulaires|pdfjs|prefilled_f1040.pdf|
formulaires|pdfjs|annotation-text-widget.pdf|
formulaires|pdfjs|annotation-choice-widget.pdf|
formulaires|pdfjs|annotation-button-widget.pdf|
formulaires|pdfjs|acroform_calculation_order.pdf|
formulaires|pdfjs|xfa_issue14315.pdf|
formulaires|qpdf|form-filled-with-atril.pdf|
formulaires|qpdf|form-no-need-appearances.pdf|
formulaires|qpdf|form-fields-and-annotations.pdf|
formulaires|qpdf|annotations-no-acroform.pdf|
annotations|pdfjs|annotation-highlight.pdf|
annotations|pdfjs|annotation-highlight-without-appearance.pdf|
annotations|pdfjs|annotation-line.pdf|
annotations|pdfjs|annotation-line-without-appearance.pdf|
annotations|pdfjs|annotation-square-circle.pdf|
annotations|pdfjs|annotation-square-circle-without-appearance.pdf|
annotations|pdfjs|annotation-freetext.pdf|
annotations|pdfjs|annotation-fileattachment.pdf|
annotations|pdfjs|annotation-stamp.pdf|
annotations|pdfjs|annotation-caret-ink.pdf|
annotations|pdfjs|annotation-ink-without-appearance.pdf|
annotations|pdfjs|annotation-polyline-polygon.pdf|
annotations|pdfjs|annotation-link-text-popup.pdf|
annotations|pdfjs|tagged_stamp.pdf|
images|pdfjs|cmykjpeg.pdf|
images|pdfjs|perf_codecs_jpeg_cmyk.pdf|
images|pdfjs|ccitt_EndOfBlock_false.pdf|
images|pdfjs|perf_codecs_ccitt.pdf|
images|pdfjs|perf_codecs_jbig2.pdf|
images|pdfjs|perf_codecs_jpx.pdf|
images|pdfjs|perf_images_scan_bilevel.pdf|
images|pdfjs|scan-bad.pdf|
images|pdfjs|bug_jpx.pdf|
images|pdfjs|jpx_smaskindata.pdf|
images|pdfjs|alphatrans.pdf|
images|pdfjs|devicen.pdf|
images|qpdf|encrypted-with-images.pdf|
images|qpdf|damaged-stream.pdf|
polices|pdfjs|ArabicCIDTrueType.pdf|
polices|pdfjs|arial_unicode_ab_cidfont.pdf|
polices|pdfjs|cid_cff.pdf|
polices|pdfjs|vertical.pdf|
polices|pdfjs|perf_text_cjk_nonembedded.pdf|
polices|pdfjs|perf_text_cid_truetype.pdf|
polices|pdfjs|perf_text_truetype.pdf|
polices|pdfjs|perf_text_type1.pdf|
polices|pdfjs|perf_text_type3_bitmap.pdf|
polices|pdfjs|perf_text_type3_vector.pdf|
polices|pdfjs|simpletype3font.pdf|
polices|pdfjs|TrueType_without_cmap.pdf|
polices|pdfjs|nonembedded_type1_tounicode.pdf|
polices|pdfjs|Embedded_font.pdf|
polices|pdfjs|mmtype1.pdf|
polices|pdfjs|IdentityToUnicodeMap_charCodeOf.pdf|
polices|pdfjs|text_clip_cff_cid.pdf|
polices|pdfjs|Type3WordSpacing.pdf|
structure|pdfjs|structure_simple.pdf|
structure|pdfjs|structure_no_parent_tree.pdf|
structure|pdfjs|tracemonkey_a11y.pdf|
structure|pdfjs|perf_structure_optional_content.pdf|
structure|pdfjs|perf_structure_forms.pdf|
structure|pdfjs|perf_structure_annotations.pdf|
navigation|pdfjs|nested_outline.pdf|
navigation|pdfjs|outline_goto_action.pdf|
navigation|pdfjs|outlines_for_editor.pdf|
navigation|pdfjs|outlines_se.pdf|
navigation|pdfjs|named_dest_collision_for_editor.pdf|
navigation|pdfjs|bad-PageLabels.pdf|
navigation|pdfjs|attachment.pdf|
navigation|pdfjs|encrypted-attachment.pdf|
navigation|pdfjs|evaljs.pdf|
navigation|pdfjs|rotation.pdf|
navigation|qpdf|direct-outlines.pdf|
navigation|qpdf|add-attachments-1.pdf|
navigation|qpdf|attachment-fields.pdf|
fichier|qpdf|c-linearized.pdf|
fichier|qpdf|c-object-streams.pdf|
fichier|qpdf|c-ignore-xref-streams.pdf|
fichier|qpdf|compress-objstm-xref.pdf|
fichier|qpdf|append-page-content.pdf|
fichier|qpdf|append-xref-loop.pdf|
fichier|qpdf|badlin1.pdf|
fichier|qpdf|dangling-bad-xref.pdf|
fichier|qpdf|bad-xref.pdf|
fichier|qpdf|broken-lzw.pdf|
fichier|qpdf|bad-content.pdf|
fichier|qpdf|good17.pdf|
fichier|pdfjs|xref_command_missing.pdf|
fichier|pdfjs|GHOSTSCRIPT-698804-1-fuzzed.pdf|
chiffrement|qpdf|encrypted-40-bit-R3.pdf|
chiffrement|qpdf|encrypted-positive-P.pdf|
chiffrement|qpdf|enc-long-password.pdf|
chiffrement|qpdf|c-r6-in.pdf|
chiffrement|qpdf|c-decrypt-R6-with-owner.pdf|
chiffrement|qpdf|enc-XI-R6,V5,U=view,O=master.pdf|enc-R6-U-view-O-master.pdf
chiffrement|qpdf|enc-XI-R6,V5,U=view,attachments,cleartext-metadata.pdf|enc-R6-cleartext-metadata.pdf
chiffrement|qpdf|enc-XI-R6,V5,U=attachment,encrypted-attachments.pdf|enc-R6-encrypted-attachments.pdf
signatures|pdfjs|signed_verified.pdf|
pdfa|vera|PDF_A-1b/6.1 File structure/6.1.2 File header/veraPDF test suite 6-1-2-t01-pass-a.pdf|pdfa-1b-6-1-2-t01-pass-a.pdf
pdfa|vera|PDF_A-1b/6.2 Graphics/6.2.2 Output intent/veraPDF test suite 6-2-2-t01-pass-a.pdf|pdfa-1b-6-2-2-t01-pass-a.pdf
pdfa|vera|PDF_A-1b/6.4 Transparency/veraPDF test suite 6-4-t01-pass-a.pdf|pdfa-1b-6-4-t01-pass-a.pdf
pdfa|vera|PDF_A-1b/6.7 Metadata/6.7.2 Properties/veraPDF test suite 6-7-2-t06-pass-g.pdf|pdfa-1b-6-7-2-t06-pass-g.pdf
pdfa|vera|PDF_A-2b/6.1 File structure/6.1.12 Permissions/veraPDF test suite 6-1-12-t01-pass-a.pdf|pdfa-2b-6-1-12-t01-pass-a.pdf
pdfa|vera|PDF_A-2b/6.2 Graphics/6.2.10 Transparency/veraPDF test suite 6-2-10-t06-pass-a.pdf|pdfa-2b-6-2-10-t06-pass-a.pdf
pdfa|vera|PDF_A-2b/6.2 Graphics/6.2.11 Fonts/6.2.11.6 Character encodings/veraPDF test suite 6-2-11-6-t02-pass-d.pdf|pdfa-2b-6-2-11-6-t02-pass-d.pdf
pdfa|vera|PDF_A-2b/6.2 Graphics/6.2.8 Images/6.2.8.1 General/veraPDF test suite 6-2-8-1-t03-pass-a.pdf|pdfa-2b-6-2-8-1-t03-pass-a.pdf
pdfa|vera|PDF_A-2u/6.2 Graphics/6.2.11 Fonts/6.2.11.7 Unicode character maps/6.2.11.7.2 Level A and Level U conformance/veraPDF test suite 6-2-11-7-2-t01-pass-a.pdf|pdfa-2u-6-2-11-7-2-t01-pass-a.pdf
pdfa|vera|PDF_A-3b/6.8 Embedded files/veraPDF test suite 6-8-t02-pass-a.pdf|pdfa-3b-6-8-t02-pass-a.pdf
pdfa|vera|PDF_A-4/6.1 File structure/6.1.2 File header/veraPDF test suite 6-1-2-t01-pass-a.pdf|pdfa-4-6-1-2-t01-pass-a.pdf
pdfa|vera|PDF_A-4/6.10 Optional content/veraPDF test suite 6-10-t02-pass-a.pdf|pdfa-4-6-10-t02-pass-a.pdf
pdfa|vera|PDF_A-4/6.2 Graphics/6.2.10 Fonts/6.2.10.3 Composite fonts/6.2.10.3.2 CIDFonts/veraPDF test suite 6-2-10-3-2-t01-pass-b.pdf|pdfa-4-6-2-10-3-2-t01-pass-b.pdf
pdfa|vera|PDF_A-4/6.2 Graphics/6.2.10 Fonts/6.2.10.8 Use of ActualText/veraPDF test suite 6-2-10-8-t01-pass-a.pdf|pdfa-4-6-2-10-8-t01-pass-a.pdf
pdfa|vera|PDF_A-4/6.3 Annotations/6.3.3 Annotation appearances/veraPDF test suite 6-3-3-t01-pass-a.pdf|pdfa-4-6-3-3-t01-pass-a.pdf
pdfa|vera|PDF_A-4/6.6 Action/6.6.3 Trigger events/veraPDF test suite 6-6-3-t01-pass-e.pdf|pdfa-4-6-6-3-t01-pass-e.pdf
pdfua|vera|PDF_UA-1/7.1 General/7.1-t01-pass-a.pdf|pdfua-1-7-1-t01-pass-a.pdf
pdfua|vera|PDF_UA-1/7.10 Optional content/7.10-t02-pass-a.pdf|pdfua-1-7-10-t02-pass-a.pdf
pdfua|vera|PDF_UA-1/7.18 Annotations/7.18.1 General/7.18.1-t02-pass-a.pdf|pdfua-1-7-18-1-t02-pass-a.pdf
pdfua|vera|PDF_UA-1/7.18 Annotations/7.18.7 File Attachments/7.18.7-t01-pass-a.pdf|pdfua-1-7-18-7-t01-pass-a.pdf
pdfua|vera|PDF_UA-2/8.2 Logical structure/8.2.2 Real content/8.2.2-t01-pass-b.pdf|pdfua-2-8-2-2-t01-pass-b.pdf
pdfua|vera|PDF_UA-2/8.2 Logical structure/8.2.5 Additional requirements for specific structure types/8.2.5.26 Table (Table, TR, TH, TD, THead, TBody, TFoot)/8.2.5.26-t05-pass-b.pdf|pdfua-2-8-2-5-26-t05-pass-b.pdf
pdfua|vera|PDF_UA-2/8.4 Text representation for content/8.4.4 Declaring natural language/8.4.4-t02-pass-c.pdf|pdfua-2-8-4-4-t02-pass-c.pdf
'

printf '%s\n' "$entries" | while IFS='|' read -r family src remote local; do
  [ -z "$family" ] && continue
  case "$src" in pdfjs) base=$pdfjs ;; qpdf) base=$qpdf ;; vera) base=$vera ;; esac
  [ -z "$local" ] && local=$remote
  dir="$root/$family"; mkdir -p "$dir"; dest="$dir/$local"
  if [ -f "$dest" ] && [ "$force" = 0 ]; then printf 'déjà là  %s/%s\n' "$family" "$local"; continue; fi
  curl -fsSL -A 'MegaUltraPDF-audit/0.13' -o "$dest" "$base$(urlencode "$remote")" || { echo "ÉCHEC $family/$local"; rm -f "$dest"; continue; }
  printf '%-12s %-48s %9s o  %s\n' "$family" "$local" "$(stat -c %s "$dest")" "$(sha256sum "$dest" | cut -d' ' -f1)"
done
