# Mesures de l'audit de compatibilité (Lot 14)

Sorties CSV des bancs `scripts/audit/read-bench.sh` et `scripts/audit/write-bench.sh` sur le corpus
`fixtures/corpus/` (manifeste : `fixtures/corpus/MANIFEST.md`), archivées pour comparer les lots
suivants. Les colonnes sont décrites en tête de chaque script.

| Fichier | Moment | Contenu |
|---|---|---|
| `lecture-audit.csv` | audit, avant toute correction (31 octobre 2026, matin) | banc de lecture complet, 137 fichiers |
| `lecture-apres-B.csv` | après la partie B du Lot 14 (texte, annotations) | banc de lecture, sous-ensemble ciblé |
| `ecriture-apres-A.csv` | après la partie A (enregistrement sûr) | banc d'écriture, 47 fichiers × 4 éditions |
| `ecriture-apres-C.csv` | après la partie C (PDF/A, PDF/UA) | banc d'écriture, sous-ensemble ciblé |
| `ecriture-apres-D.csv` | après la partie D (taille) | banc d'écriture, sous-ensemble ciblé |
| `lecture-fin-lot-14.csv` | fin du Lot 14 | banc de lecture complet |
| `ecriture-fin-lot-14.csv` | fin du Lot 14 | banc d'écriture complet |

Le banc d'écriture de l'audit lui-même (avant la partie A) n'a pas été conservé : ses chiffres sont
dans `docs/audit/2026-10-31-compatibilite-pdf.md` (§3 et §5). Rejouer : voir §8 du même rapport.
