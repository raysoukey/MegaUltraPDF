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
     recouvrent d'au moins 50 % de la plus petite. Un caractère plus haut que `1,5 H` (lettrine,
     grande parenthèse) rejoint la ligne qu'il recouvre le plus, sans l'agrandir. Dans une
     ligne, on garde les **suites** dessinées d'affilée de gauche à droite, puis on les range
     par position. Deux mots superposés, comme des filigranes ou un faux gras, restent ainsi
     entiers.
   - **Segments** : un blanc de plus de `1,5 H` coupe une ligne (tabulation, colonnes).
   - **Découpage XY**, sur une pile explicite et non par récursion (un fichier malveillant ne
     peut pas épuiser la pile), avec un budget de travail borné :
     - **colonnes** si une bande verticale vide d'au moins `H` traverse le morceau et que
       chaque côté ressemble à de la **prose** : au moins 2 lignes et des segments larges d'au
       moins `12 H` (largeur médiane). Une liste « libellé : valeur », une liste numérotée ou
       une table des matières, quelle que soit sa longueur, reste ainsi lue ligne par ligne ;
     - sinon coupe au **plus grand** vide vertical entre paragraphes (au moins `0,8 H`, et au
       moins 1,5 fois le plus petit interligne du morceau, pour le double interligne). Comme on
       commence par le plus grand vide, un titre au-dessus de deux colonnes se détache d'abord,
       et les colonnes sont reconnues à l'étape suivante ;
     - sinon bloc final, lu ligne par ligne.
   - **Texte tourné** dans la page (bandeau vertical, en-tête de tableau tourné), repéré par
     l'angle de ses caractères (`/Rotate` déduit) : chaque suite garde son ordre de dessin, en
     bloc à part, après le texte horizontal.
2. **`text_boxes` et `page_text`** rendent cet ordre. Les sauts de ligne `\r\n` ajoutés par
   PDFium (sans emprise) sont écartés. La sélection, la copie et l'export texte partagent donc
   le même ordre.
3. **Numéros de ligne et de bloc** sur chaque `TextBox` : l'interface ne devine plus la mise en
   page. `charIndexAt` vise le **bloc** le plus proche, puis la ligne la plus proche
   verticalement, puis le caractère le plus proche. La copie met un saut de ligne à chaque
   ligne, une ligne vide à chaque bloc, comme l'export texte, et une espace entre un libellé et
   sa valeur.

## Limites connues

- Un tableau dont **toutes** les colonnes contiennent de longs textes (12 H et plus) peut être
  lu colonne par colonne.
- Des paragraphes dont le vide coïncide d'une colonne à l'autre, **au-dessus** d'un vide de titre
  plus petit, peuvent être lus par bandes (limite classique du découpage XY).
- Le texte **incliné** de plus d'environ 0,5° (scan redressé par un autre OCR, avec un caractère
  par position) peut fusionner des lignes. L'OCR de l'application produit des lignes droites.
- Texte tourné : il garde son ordre de dessin, sans analyse de lignes. Les écritures de droite à
  gauche ne sont pas traitées. Les pages tournées par `/Rotate` le sont : l'ordre est calculé en
  espace affiché.
- Un texte à l'échelle horizontale nulle (`0 Tz`) n'apparaît pas du tout dans la couche de
  PDFium : ni la sélection, ni le contrôle après caviardage ne le voient. Cette limite existait
  avant ce correctif.
- PDFium supprime parfois une espace réelle (deux espaces consécutives entre deux objets texte).
  La copie la remet d'après le blanc mesuré.

## Conséquences

- Un glisser d'une ligne A à une ligne B sélectionne exactement les lignes A à B, dans l'ordre où
  on les lit.
- Les tests qui supposaient l'ordre de dessin ont été corrigés. Exemple : un en-tête est
  désormais lu avant le corps de la page.
