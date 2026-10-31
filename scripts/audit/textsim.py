# SPDX-License-Identifier: GPL-3.0-or-later
# Similarité de deux extractions de texte (audit de compatibilité) : indice de Jaccard sur les mots,
# après normalisation (minuscules, ponctuation retirée, ligatures et espaces insécables repliés).
# Usage : python -I scripts/audit/textsim.py a.txt b.txt            → Jaccard (100 = mêmes mots)
#         python -I scripts/audit/textsim.py --contient a.txt b.txt → part des mots de a présents dans b
import re
import sys
import unicodedata


def words(path):
    try:
        text = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        return set()
    text = unicodedata.normalize("NFKC", text).lower()
    text = re.sub(r"[\W_]+", " ", text)
    return set(text.split())


contient = sys.argv[1] == "--contient"
args = sys.argv[2:] if contient else sys.argv[1:]
a, b = words(args[0]), words(args[1])
if not a and not b:
    print("100.00")
elif contient and not a:
    print("100.00")  # rien à conserver
elif not a or not b:
    print("0.00")
elif contient:
    print(f"{100 * len(a & b) / len(a):.2f}")
else:
    print(f"{100 * len(a & b) / len(a | b):.2f}")
