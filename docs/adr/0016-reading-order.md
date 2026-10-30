# ADR-0016 — Ordre de lecture de la couche texte

Date : 2026-10-30 · Statut : accepté (correctif 0.12.2)

## Contexte

La sélection de texte est une plage d'index sur les caractères de la page, et la copie suit cet
ordre. Jusqu'ici, l'ordre était celui que donne PDFium, c'est-à-dire l'**ordre de dessin** du
fichier. Sur les PDF réels, cet ordre diffère souvent de l'ordre de lecture :
- un mot en gras dessiné à la fin ;
- la colonne des valeurs d'une liste dessinée après celle des libellés ;
- un en-tête dessiné après le corps ;
- deux colonnes dessinées ligne à ligne en alternance.

Sur un document de l'utilisateur, l'ordre remontait 10 fois vers le haut de la page. Un glisser
de 3 lignes en sélectionnait 26. En plus, le choix du caractère sous la souris (centre le plus
proche) faisait sauter la sélection dans l'autre colonne dès qu'on dépassait la fin d'une ligne.

## Décisions

1. **Ordre de lecture calculé par la géométrie** (`mega-core/src/reading_order.rs`, fonction
   pure sur des rectangles). Toutes les mesures sont relatives à la hauteur médiane `H` des
   caractères.
   - **Lignes** : deux caractères sont sur la même ligne si leurs étendues verticales se
     recouvrent d'au moins 50 % de la plus petite. Dans une ligne, on garde les **suites**
     dessinées d'affilée de gauche à droite, puis on les range par position. Deux mots
     superposés, comme des filigranes ou un faux gras, restent ainsi entiers.
   - **Segments** : un blanc de plus de `1,5 H` coupe une ligne (tabulation, colonnes).
   - **Découpage XY récursif** :
     - colonnes si une bande verticale vide d'au moins `H` traverse le bloc et que chaque côté
       compte au moins **4 lignes** ;
     - sinon paragraphes, aux vides verticaux d'au moins `0,8 H` ;
     - sinon bloc final, lu ligne par ligne.
2. **`text_boxes` et `page_text`** rendent cet ordre. Les sauts de ligne `\r\n` ajoutés par
   PDFium (sans emprise) sont écartés. La sélection, la copie et l'export texte partagent donc
   le même ordre.
3. **Visée par ligne** côté interface (`charIndexAt`) : la ligne la plus proche verticalement
   d'abord, puis le caractère le plus proche sur cette ligne. La copie remet une espace entre un
   libellé et sa valeur, et une ligne vide entre deux paragraphes.

## Limites connues

- Une liste ou un tableau de **4 lignes ou plus** dont les colonnes sont séparées par un large
  blanc se lit colonne par colonne. Le seuil de 4 garde les petites listes « libellé : valeur »
  en lecture par ligne.
- Le texte vertical ou tourné **à l'intérieur** de la page, et les écritures de droite à gauche,
  ne sont pas traités. Les pages tournées par `/Rotate` le sont : l'ordre est calculé en espace
  affiché.
- PDFium supprime parfois une espace réelle (deux espaces consécutives entre deux objets texte).
  La copie la remet d'après le blanc mesuré.

## Conséquences

- Un glisser d'une ligne A à une ligne B sélectionne exactement les lignes A à B, dans l'ordre où
  on les lit.
- Les tests qui supposaient l'ordre de dessin ont été corrigés. Exemple : un en-tête est
  désormais lu avant le corps de la page.
