# Corpus de PDF réels de l'audit de compatibilité

Ce dossier (`fixtures/corpus/`) est **ignoré par Git** : seul ce manifeste est commité. Les fichiers se
récupèrent avec `bash scripts/audit/fetch-corpus.sh` (sources publiques) et se régénèrent avec les scripts
`scripts/audit/gen-*.sh|ps1` (fichiers produits sur ce poste). Chaque fichier est identifié par sa somme SHA-256
pour que les mesures du rapport `docs/audit/2026-10-31-compatibilite-pdf.md` soient rejouables.

## Sources et licences

| Source | Dépôt | Licence |
|---|---|---|
| suite de test pdf.js (Mozilla) | https://github.com/mozilla/pdf.js/tree/master/test/pdfs | Apache-2.0 |
| suite de test qpdf | https://github.com/qpdf/qpdf/tree/main/qpdf/qtest/qpdf | Apache-2.0 |
| corpus veraPDF | https://github.com/veraPDF/veraPDF-corpus | CC BY 4.0 |
| `local/` | Edge 156.0.4314 et Chrome 156.0.8078 (impression PDF headless `--print-to-pdf` de `scripts/audit/html/*.html`), Tesseract 5.4 (`tesseract … pdf` sur une page rendue à 200 dpi), Word (COM, `scripts/audit/gen-word.ps1`) | produits pour ce dépôt |

## Familles

- **producteurs** : sorties de vrais logiciels (pdfTeX, Word, OpenOffice/LibreOffice, Acrobat, Ghostscript…).
- **formulaires** : AcroForm réels (IRS f1040, 160F), widgets de chaque type, un XFA.
- **annotations** : chaque sous-type, avec et sans apparence `/AP`, popups, pièce jointe.
- **images** : JPEG CMJN, CCITT, JBIG2, JPX, scan bilevel, transparence, DeviceN, flux abîmés.
- **polices** : CID TrueType arabe, CFF CID, vertical, CJK non incorporé, Type1, Type3, TrueType sans cmap, MM.
- **structure** : balisage (`/StructTreeRoot`), calques (`/OCProperties`), formulaires XObject.
- **navigation** : signets imbriqués, actions GoTo, destinations nommées, `/PageLabels`, pièces jointes, JavaScript, rotations.
- **fichier** : linéarisé, flux d'objets, xref stream, révisions incrémentales, xref abîmées, LZW cassé.
- **chiffrement** : RC4 40 bits R3, AES-256 R6 (mot de passe utilisateur et propriétaire), pièces jointes chiffrées, métadonnées en clair.
- **signatures** : une signature vérifiée.
- **pdfa** / **pdfua** : fichiers `pass` du corpus veraPDF (PDF/A-1b, 2b, 2u, 3b, 4 ; PDF/UA-1, 2).
- **local** : documents produits sur ce poste par de vrais logiciels grand public.

Mots de passe connus (suite qpdf, lus dans le nom du fichier) : `enc-R6-U-view-O-master.pdf` → `view` / `master` ;
`enc-R6-cleartext-metadata.pdf` → `view` ; `enc-R6-encrypted-attachments.pdf` → `attachment` ;
`enc-long-password.pdf` → `wwwww…` (long) ; les autres fichiers chiffrés de qpdf servent à tester le **refus** (mot de passe
inconnu). Le banc `scripts/audit/read-bench.sh` essaie une liste de candidats avec qpdf.

## Fichiers

| Fichier | Octets | Source | Nom distant | SHA-256 |
|---|---|---|---|---|
| `annotations/annotation-caret-ink.pdf` | 93 680 | suite de test pdf.js |  | `afdc6fb72e8dcb8f0796a48388fc011e9db212e18d1d7b8cf95b327e88cf6ce9` |
| `annotations/annotation-fileattachment.pdf` | 93 322 | suite de test pdf.js |  | `b2f3751cd63e2402d95673f763e52fb105e6ce8e0b41406ca6d9965f5c894d43` |
| `annotations/annotation-freetext.pdf` | 86 024 | suite de test pdf.js |  | `318ed4bead93fa6ff023571b17ddca41e82fe2347b497ea24903a11e1892fb71` |
| `annotations/annotation-highlight.pdf` | 92 397 | suite de test pdf.js |  | `72b6387356ccfdfb95a0a5eff5e1ceb9b7667019807d7b04abaf64dad7ea143a` |
| `annotations/annotation-highlight-without-appearance.pdf` | 16 302 | suite de test pdf.js |  | `ea323f17899de03e7499d0e8531c49e873512a26b40058f357ca399312bf65d6` |
| `annotations/annotation-ink-without-appearance.pdf` | 6 288 | suite de test pdf.js |  | `663c7419206b1995c6ae651a1cbf8dbf9d89f0f6b3e61014cf545e21335e75b8` |
| `annotations/annotation-line.pdf` | 84 943 | suite de test pdf.js |  | `cea9ed8cf5726fa643da0c083bc8db8df790ab5e7ee9a2b62224aab73d9fa742` |
| `annotations/annotation-line-without-appearance.pdf` | 14 170 | suite de test pdf.js |  | `9408cc19a56ef122ea956799a2e5a63b2b3cc70bbd307ed76d2ba89fd9c54fc3` |
| `annotations/annotation-link-text-popup.pdf` | 98 997 | suite de test pdf.js |  | `06917202feda6b6bb8b57b048655f946f749bdff5a8bfcb35eb0116f38910312` |
| `annotations/annotation-polyline-polygon.pdf` | 88 691 | suite de test pdf.js |  | `66303646dd550366994f6715678863536617093335825dda3c123616555b69e2` |
| `annotations/annotation-square-circle.pdf` | 88 740 | suite de test pdf.js |  | `88ed9ed813c3585b3098b9f4eef5107426f7829044117cd87f77bdc38a8e3f72` |
| `annotations/annotation-square-circle-without-appearance.pdf` | 16 007 | suite de test pdf.js |  | `d0a98bb14150b1bc2bc028876198098c99420ea10d30d43b4f9c5cb36bb023a7` |
| `annotations/annotation-stamp.pdf` | 88 903 | suite de test pdf.js |  | `76d566c092c23ccbec4bb488c40dc6daec8a381cb5386452b186728652436704` |
| `annotations/tagged_stamp.pdf` | 18 885 | suite de test pdf.js |  | `0ce2c20e36b70558eea45511a2c307ce7e22ad0d51be1216b27f7085c0311198` |
| `chiffrement/c-decrypt-R6-with-owner.pdf` | 9 731 | suite de test qpdf |  | `db81c94f5a8f6c147ae6bfef134a5c483a2ef7cfc33dea8559dbe8ed5825a8df` |
| `chiffrement/c-r6-in.pdf` | 11 206 | suite de test qpdf |  | `b616425b595e376dbbfc19dbb89a66678afd4eb74f85575f667778afa1add892` |
| `chiffrement/enc-long-password.pdf` | 17 144 | suite de test qpdf |  | `2d9b4e2d030f48b463cc7c1acb399db0d6a5869250b0933ddf675bb2cea84c48` |
| `chiffrement/enc-R6-cleartext-metadata.pdf` | 22 400 | suite de test qpdf | `enc-XI-R6 V5 U=view attachments cleartext-metadata.pdf` | `d0d7d6d37a4c1e551dabb45532b3b077d973cf8f84ac04d865fd9a5e5fca1d68` |
| `chiffrement/enc-R6-encrypted-attachments.pdf` | 17 868 | suite de test qpdf | `enc-XI-R6 V5 U=attachment encrypted-attachments.pdf` | `814df59a61f34da60df347ab1a268b6375d2b0de9058d33200d00b4b34e59a66` |
| `chiffrement/enc-R6-U-view-O-master.pdf` | 15 670 | suite de test qpdf | `enc-XI-R6 V5 U=view O=master.pdf` | `96b9262bfacf1358c7a68ab46c11563677e858888950237fdac8f3a227210edf` |
| `chiffrement/encrypted-40-bit-R3.pdf` | 1 041 | suite de test qpdf |  | `221b17165d2c807500165c7890f6f8f355b6cf62ecdb430d9bae71cec17b8369` |
| `chiffrement/encrypted-positive-P.pdf` | 1 488 | suite de test qpdf |  | `e717ab57a216202dc669f9cbdc335fb863b105c400a4b946594a456f4aaf3380` |
| `fichier/append-page-content.pdf` | 17 370 | suite de test qpdf |  | `2a70474d0185222b0cb22089de654f8f2a4d207b7f61b646a030b1e563da837b` |
| `fichier/append-xref-loop.pdf` | 17 370 | suite de test qpdf |  | `7c67daccc433f574fbaf815d0d376cc79f40f95082ef1b231af436e9f7d27091` |
| `fichier/bad-content.pdf` | 135 172 | suite de test qpdf |  | `e1a6ebaadd386a0b1e638e3b50c8557da74ebeb1ebb31292fb6b221892a979e0` |
| `fichier/badlin1.pdf` | 13 103 | suite de test qpdf |  | `236e5197c21f3dd1b032951d205156988b788e4ce1e36d16cd17a1c5781a0799` |
| `fichier/bad-xref.pdf` | 819 | suite de test qpdf |  | `5895dfedf978f600b12e29643617dbb111869b60526d957878e349571e1467d1` |
| `fichier/broken-lzw.pdf` | 1 119 | suite de test qpdf |  | `cf51252799ff8a7d083760559dc003031f790a4f2caf6f064e8c00b3484e3466` |
| `fichier/c-ignore-xref-streams.pdf` | 9 976 | suite de test qpdf |  | `cd5097b5a100247b008d751e90fa3a5c33766d25c1740fd35c40791e7e6f5315` |
| `fichier/c-linearized.pdf` | 11 325 | suite de test qpdf |  | `135cee6bd018c06d40c8c46c4a0230b2799adb90f502e214a347b3855039a24e` |
| `fichier/c-object-streams.pdf` | 5 195 | suite de test qpdf |  | `0588a730f7b499b2245e826fcba3df9131f698062e774ae2a4fce2db1d67cf0a` |
| `fichier/compress-objstm-xref.pdf` | 743 | suite de test qpdf |  | `7aa4aea24328616ce247cc4132efa3b8f53d2844179d17b51383f8771d2de358` |
| `fichier/dangling-bad-xref.pdf` | 1 097 | suite de test qpdf |  | `a6cb86b3a588674544eebdd78523e21c870055267c0a54da39275211b1a0ad2e` |
| `fichier/GHOSTSCRIPT-698804-1-fuzzed.pdf` | 627 | suite de test pdf.js |  | `62e518ca7894e8b8b8ed9927da3007cdc5e9ccc5b111bbb22d6101d4a8d7490e` |
| `fichier/good17.pdf` | 17 031 | suite de test qpdf |  | `7383ca2c3a1da7c0691e0b8a63ebee3c91b62041ee5a3433a69e721b6c48f190` |
| `fichier/xref_command_missing.pdf` | 631 | suite de test pdf.js |  | `88d3905774607b3f529e369db122b2d14051417993c9a704472c91c6702ccff6` |
| `formulaires/160F-2019.pdf` | 328 400 | suite de test pdf.js |  | `f41f0818365ffd14ca4e55c0556355193bd76632b970d60cc2dec554cac6f379` |
| `formulaires/acroform_calculation_order.pdf` | 1 041 | suite de test pdf.js |  | `9081aad78cb6bb1871eb5c51addbe75ce73e4617a3e1fa15aa3da21232b6b755` |
| `formulaires/annotation-button-widget.pdf` | 23 785 | suite de test pdf.js |  | `f7b1c9c7b2ecb909f6b168cc1b137230076533f977a205472b212b7f0d3a78d7` |
| `formulaires/annotation-choice-widget.pdf` | 91 556 | suite de test pdf.js |  | `2fcab2aed82afe9775359c1d166fb7aae77e99d25fe03be177e233d02d572538` |
| `formulaires/annotations-no-acroform.pdf` | 9 559 | suite de test qpdf |  | `81802644c8ea1535d00edca9719a476498ee8d5d82988edb10a4e63b386f2d76` |
| `formulaires/annotation-text-widget.pdf` | 93 171 | suite de test pdf.js |  | `9f091ceb368ad0406094efaa6c50d5033735ee52f383e0973dc53a7467089d7c` |
| `formulaires/form-fields-and-annotations.pdf` | 9 648 | suite de test qpdf |  | `2288db9af745217c054f167cd5c416eb80e3dba98097da104d6b77c2a150912a` |
| `formulaires/form-filled-with-atril.pdf` | 56 705 | suite de test qpdf |  | `25a5d4761723754c37d39f0a46f123b0603408a866e7a45e48856eb5a0c09b12` |
| `formulaires/form-no-need-appearances.pdf` | 12 572 | suite de test qpdf |  | `72a909d80238419f3509af4d0765b2ac882c1f15d7b27fe8ad1d959c0baa5bb0` |
| `formulaires/prefilled_f1040.pdf` | 306 416 | suite de test pdf.js |  | `899edb271684d2451337e225268003324a583a7d3429e23efd69f1654adf9bf9` |
| `formulaires/xfa_issue14315.pdf` | 11 568 | suite de test pdf.js |  | `d039a40ea28384bae8d1a24929a4288aae1aeb433c441fd39c04fad618fa6f19` |
| `images/alphatrans.pdf` | 16 910 | suite de test pdf.js |  | `234c0ab8e9458f4d17133b08d2b66170eeb92be2a80e1508aa5117a2bf568f63` |
| `images/bug_jpx.pdf` | 5 451 | suite de test pdf.js |  | `4010d808557a72278368c05518367735ed186cbdd04974b8506275f9ac40f999` |
| `images/ccitt_EndOfBlock_false.pdf` | 3 256 | suite de test pdf.js |  | `f2289b94e7e3f05f9e37754c399d3f525edf9444fb32c3279959cf64b575fe62` |
| `images/cmykjpeg.pdf` | 374 080 | suite de test pdf.js |  | `659d6b19912f63db988b0b26b9bde0e6d8100667ef162051a4a84ea8e5b90272` |
| `images/damaged-stream.pdf` | 77 635 | suite de test qpdf |  | `91e119f2db2e9f738f696ddacea90498d460b0afda0fcdf71bb6281fc5085819` |
| `images/devicen.pdf` | 15 751 | suite de test pdf.js |  | `1fbaf094e1c714513aea19258e74269b6b5365ea769e288a8e142773f6cf2b94` |
| `images/encrypted-with-images.pdf` | 50 219 | suite de test qpdf |  | `9f6c9ad93a462ad8a9c3c28debd5cf6ab80c60e7a8fe3b12e802b8e53aa0b1d4` |
| `images/jpx_smaskindata.pdf` | 2 515 | suite de test pdf.js |  | `a82813559c74f074b0caf92c80875fc018acb737269a2af5508c7e083b2a5fd3` |
| `images/perf_codecs_ccitt.pdf` | 38 113 | suite de test pdf.js |  | `8b262e6df39f1d66151830911b1371ccdb98cedc53b50589a209143293c98221` |
| `images/perf_codecs_jbig2.pdf` | 9 909 | suite de test pdf.js |  | `c7cdfb036282939f6a5461d67c34c895910856266c598f118cc41c9b7ca1d82e` |
| `images/perf_codecs_jpeg_cmyk.pdf` | 829 883 | suite de test pdf.js |  | `d6bac90d35bd5bce1d6f43071ba1fcd56f19189c5c697e37cf40e9dce1f5a0be` |
| `images/perf_codecs_jpx.pdf` | 524 879 | suite de test pdf.js |  | `0f8479d461d0d7e5fb3875f8336ecbbc995ec180bc60b07a7a8be757a9e22169` |
| `images/perf_images_scan_bilevel.pdf` | 124 234 | suite de test pdf.js |  | `3fabd640596805188db89117c50613e1897502329ac16ac4cab8a1049fb86ad8` |
| `images/scan-bad.pdf` | 29 832 | suite de test pdf.js |  | `d40f31f182cf254ab134826d379875acacb1f7b48bf1a57bce2e30b38d16628e` |
| `local/chrome-article.pdf` | 156 517 | généré sur ce poste (scripts/audit/) |  | `16bc7d6512a3bea437dd40e17b936f775a96bd71b357569d770af1498704cef0` |
| `local/chrome-formulaire.pdf` | 49 461 | généré sur ce poste (scripts/audit/) |  | `b1aa94f87fe274ba1d19faec5d61f9e5bf7eaae42520229fbca735969bd51277` |
| `local/edge-article.pdf` | 156 531 | généré sur ce poste (scripts/audit/) |  | `e8ef8113f39a7a18eaedaad9ca617eb9d4ebf56b13aef25920483c8b237c1ec5` |
| `local/edge-formulaire.pdf` | 49 475 | généré sur ce poste (scripts/audit/) |  | `bcbc29162802d0e2e60cb83236376d6bbbacef7a5e6431c2be49f476d51a1583` |
| `local/tesseract-scan.pdf` | 188 973 | généré sur ce poste (scripts/audit/) |  | `6869decb13530648530217bfac8a5f2c993c23f60a6701f9413f96ada16033ac` |
| `navigation/add-attachments-1.pdf` | 2 602 | suite de test qpdf |  | `2332893b6b30c04d37c976d7d79b80a7fc7c925aae094a93e9bde2f751d8efbd` |
| `navigation/attachment.pdf` | 15 299 | suite de test pdf.js |  | `9acec9f83f1557183c83a87111b85141e657849ac2aa92864ffd63cc91572379` |
| `navigation/attachment-fields.pdf` | 2 242 | suite de test qpdf |  | `7dff78a9b17d4a9381c42090bca248a31054b15e12a993f3e2d58ad75426ee71` |
| `navigation/bad-PageLabels.pdf` | 798 | suite de test pdf.js |  | `eff0ee7d1cf2e1a7ccfb25a8fc4c2bbea0ec23c838b61a199a57dbb732ddeaad` |
| `navigation/direct-outlines.pdf` | 15 994 | suite de test qpdf |  | `afd980c57c22d242db3126f24b85a7699d4674c2502a46fc0e2219fb1b0bbefe` |
| `navigation/encrypted-attachment.pdf` | 2 719 | suite de test pdf.js |  | `0f44da5152adf8cafcd7b0057d840310d78e17d033ee5414da0881b6dcd130ab` |
| `navigation/evaljs.pdf` | 55 450 | suite de test pdf.js |  | `dad709ec8c7197fee70bcf053dcf8609d4f05e1830c96c4af51c85e83957c995` |
| `navigation/named_dest_collision_for_editor.pdf` | 811 | suite de test pdf.js |  | `c82bcb3934f24c82a80d52838ab90e2b2d0fca7b5c2973e5a482123fe1b84949` |
| `navigation/nested_outline.pdf` | 5 929 | suite de test pdf.js |  | `1283aa2173bc57def82323ea2601a4e08ada5beb2466873aec8aa04ddd74c013` |
| `navigation/outline_goto_action.pdf` | 1 061 | suite de test pdf.js |  | `7b653d3e95ca550509abcfd3887754d2ec0c68da330ec03a95c77bdb47479ecc` |
| `navigation/outlines_for_editor.pdf` | 2 982 | suite de test pdf.js |  | `084e3b2eb28f5d512f12cd3871bb8b161226f5ee43c82f89c7c5ee0987f58d1e` |
| `navigation/outlines_se.pdf` | 30 444 | suite de test pdf.js |  | `11d268780724faf63110612a4625923420d2f60075c07d5365d2c9197d1c04cd` |
| `navigation/rotation.pdf` | 6 605 | suite de test pdf.js |  | `5087c2afe763d145537d652bb3bc6c321ace3a267b2679ae2c178add3ba7f1d9` |
| `pdfa/pdfa-1b-6-1-2-t01-pass-a.pdf` | 4 048 | corpus veraPDF | `PDF_A-1b/6.1 File structure/6.1.2 File header/veraPDF test suite 6-1-2-t01-pass-a.pdf` | `66077f449d472a048e3bbf7192aa6d2b0b0ebd6b6d8a6f878f776f69424b6deb` |
| `pdfa/pdfa-1b-6-2-2-t01-pass-a.pdf` | 3 024 | corpus veraPDF | `PDF_A-1b/6.2 Graphics/6.2.2 Output intent/veraPDF test suite 6-2-2-t01-pass-a.pdf` | `97e30bd4477b02f139dfed1613346a09491babd3d9297d989df5829c2ecd1a48` |
| `pdfa/pdfa-1b-6-4-t01-pass-a.pdf` | 9 527 | corpus veraPDF | `PDF_A-1b/6.4 Transparency/veraPDF test suite 6-4-t01-pass-a.pdf` | `49ae6e0dd61c8dffb903e77f7d103e3ba1a8fbfd6955282465d2b3b1fdd3c928` |
| `pdfa/pdfa-1b-6-7-2-t06-pass-g.pdf` | 3 501 | corpus veraPDF | `PDF_A-1b/6.7 Metadata/6.7.2 Properties/veraPDF test suite 6-7-2-t06-pass-g.pdf` | `98b77544192f5a13c330d16106e076c11f92656fd6a79b9ef21982e056e463a6` |
| `pdfa/pdfa-2b-6-1-12-t01-pass-a.pdf` | 6 706 | corpus veraPDF | `PDF_A-2b/6.1 File structure/6.1.12 Permissions/veraPDF test suite 6-1-12-t01-pass-a.pdf` | `f389929bb730a7f0dab85a7179c76fc7f4cfc4a1f1f1210d2acc05d8dd99bf6b` |
| `pdfa/pdfa-2b-6-2-10-t06-pass-a.pdf` | 4 391 | corpus veraPDF | `PDF_A-2b/6.2 Graphics/6.2.10 Transparency/veraPDF test suite 6-2-10-t06-pass-a.pdf` | `54a88f9d654f6fc294a3b962014a3699a334588770be5b0036092c5b397ac76e` |
| `pdfa/pdfa-2b-6-2-11-6-t02-pass-d.pdf` | 9 767 | corpus veraPDF | `PDF_A-2b/6.2 Graphics/6.2.11 Fonts/6.2.11.6 Character encodings/veraPDF test suite 6-2-11-6-t02-pass-d.pdf` | `92fd3cd8ad0ab7d29903ddc43ba73a75de77b5703d2fd912db3123855c889708` |
| `pdfa/pdfa-2b-6-2-8-1-t03-pass-a.pdf` | 21 793 | corpus veraPDF | `PDF_A-2b/6.2 Graphics/6.2.8 Images/6.2.8.1 General/veraPDF test suite 6-2-8-1-t03-pass-a.pdf` | `9381c56e144a4a3e19b9dd0f3bbae58eee69149adea81e63f929c4b535a542a5` |
| `pdfa/pdfa-2u-6-2-11-7-2-t01-pass-a.pdf` | 10 443 | corpus veraPDF | `PDF_A-2u/6.2 Graphics/6.2.11 Fonts/6.2.11.7 Unicode character maps/6.2.11.7.2 Level A and Level U conformance/veraPDF test suite 6-2-11-7-2-t01-pass-a.pdf` | `312a49dc835643d312960983251df85f747a12d357013ee3426b30c4dd5aaed0` |
| `pdfa/pdfa-3b-6-8-t02-pass-a.pdf` | 4 976 | corpus veraPDF | `PDF_A-3b/6.8 Embedded files/veraPDF test suite 6-8-t02-pass-a.pdf` | `5b8fdee7090f0a0fb7266d10dd0f6f1f8a37d286e3c06b1aa82e01d6d4f0a8a4` |
| `pdfa/pdfa-4-6-10-t02-pass-a.pdf` | 14 406 | corpus veraPDF | `PDF_A-4/6.10 Optional content/veraPDF test suite 6-10-t02-pass-a.pdf` | `047b3c080c2822a4951b8792bb6240a536627906ff8a818bde5f3c0539eca4e6` |
| `pdfa/pdfa-4-6-1-2-t01-pass-a.pdf` | 3 251 | corpus veraPDF | `PDF_A-4/6.1 File structure/6.1.2 File header/veraPDF test suite 6-1-2-t01-pass-a.pdf` | `97814be08003c207e1bed44fc49df78afa460660e82708bd17ca26b626a6e4ec` |
| `pdfa/pdfa-4-6-2-10-3-2-t01-pass-b.pdf` | 35 741 | corpus veraPDF | `PDF_A-4/6.2 Graphics/6.2.10 Fonts/6.2.10.3 Composite fonts/6.2.10.3.2 CIDFonts/veraPDF test suite 6-2-10-3-2-t01-pass-b.pdf` | `72437019bdb177fa742ffa02bdb379c88f3208020e69fe238e312d87368bd343` |
| `pdfa/pdfa-4-6-2-10-8-t01-pass-a.pdf` | 3 471 | corpus veraPDF | `PDF_A-4/6.2 Graphics/6.2.10 Fonts/6.2.10.8 Use of ActualText/veraPDF test suite 6-2-10-8-t01-pass-a.pdf` | `f723457f62f1d08d91fd9ab941e88e99834193837a3ec44e32e91846c7ef8acb` |
| `pdfa/pdfa-4-6-3-3-t01-pass-a.pdf` | 3 175 | corpus veraPDF | `PDF_A-4/6.3 Annotations/6.3.3 Annotation appearances/veraPDF test suite 6-3-3-t01-pass-a.pdf` | `137ef5c6c24dd8e920bbf9153b8d702c29afe80542a5f800bf1e36247647008a` |
| `pdfa/pdfa-4-6-6-3-t01-pass-e.pdf` | 3 813 | corpus veraPDF | `PDF_A-4/6.6 Action/6.6.3 Trigger events/veraPDF test suite 6-6-3-t01-pass-e.pdf` | `66a502927a0e8521a8846a06fcd4c6d45ff479c517578054375e7c77d94c707a` |
| `pdfua/pdfua-1-7-10-t02-pass-a.pdf` | 70 971 | corpus veraPDF | `PDF_UA-1/7.10 Optional content/7.10-t02-pass-a.pdf` | `47682c57ccace511dc2fddf95bd745b13584ae25c4e07d7b8711c295d6d8c7b4` |
| `pdfua/pdfua-1-7-18-1-t02-pass-a.pdf` | 20 226 | corpus veraPDF | `PDF_UA-1/7.18 Annotations/7.18.1 General/7.18.1-t02-pass-a.pdf` | `12b4e06019bb5e1765d0bc371df16c84635a1e5145f2ecb5fba51c17245ebc51` |
| `pdfua/pdfua-1-7-18-7-t01-pass-a.pdf` | 106 022 | corpus veraPDF | `PDF_UA-1/7.18 Annotations/7.18.7 File Attachments/7.18.7-t01-pass-a.pdf` | `5792f5da673fcd4e9f75d9776922a609edfb2010ec4cc2d4773f5c657803cf34` |
| `pdfua/pdfua-1-7-1-t01-pass-a.pdf` | 72 296 | corpus veraPDF | `PDF_UA-1/7.1 General/7.1-t01-pass-a.pdf` | `cc57668afd86160fafa324e2e6c57ce2cb8dc087778b7c74559c85fa52f3a846` |
| `pdfua/pdfua-2-8-2-2-t01-pass-b.pdf` | 17 028 | corpus veraPDF | `PDF_UA-2/8.2 Logical structure/8.2.2 Real content/8.2.2-t01-pass-b.pdf` | `2a548a894cfdbc5a752c1de7b039fbafa8d249158b9d05166a4fa00589e17c6c` |
| `pdfua/pdfua-2-8-2-5-26-t05-pass-b.pdf` | 42 229 | corpus veraPDF | `PDF_UA-2/8.2 Logical structure/8.2.5 Additional requirements for specific structure types/8.2.5.26 Table (Table  TR  TH  TD  THead  TBody  TFoot)/8.2.5.26-t05-pass-b.pdf` | `26396e7544976587b4ea8d4f77df73658b32d16dfbfab1a39dbf5f0295eedb23` |
| `pdfua/pdfua-2-8-4-4-t02-pass-c.pdf` | 18 016 | corpus veraPDF | `PDF_UA-2/8.4 Text representation for content/8.4.4 Declaring natural language/8.4.4-t02-pass-c.pdf` | `3190c5c09175fef385cd81fce439d85c6d1b2bbca127e4c921c238dfe3faf661` |
| `polices/ArabicCIDTrueType.pdf` | 39 370 | suite de test pdf.js |  | `1cad1de912ba29f89a6d8b08bc5b0f84382874ffedab2c8f9e05ef608c265bb1` |
| `polices/arial_unicode_ab_cidfont.pdf` | 13 110 | suite de test pdf.js |  | `049a4330710d8470896061acf7ee486735e94cef2769f6cc13fec3cb13bb65aa` |
| `polices/cid_cff.pdf` | 19 970 | suite de test pdf.js |  | `d894d356411217414fd9040d3d038612e1f4a1598936b2385a76458dd6d64d38` |
| `polices/Embedded_font.pdf` | 3 323 | suite de test pdf.js |  | `4b7588e070cd37c7cfa2ce0a493ba9ec6d57fb36a2ca3c6ab50c74a7818a2f9a` |
| `polices/IdentityToUnicodeMap_charCodeOf.pdf` | 651 | suite de test pdf.js |  | `81243ff4cca5f7898688008224247d88534cd03e76ac3cbbdc38f925cfbee18f` |
| `polices/mmtype1.pdf` | 8 044 | suite de test pdf.js |  | `6e08f0e3f64ed302f00572e2c4e391614df98cc9fa9d5e40538d495aa1a2063c` |
| `polices/nonembedded_type1_tounicode.pdf` | 1 175 | suite de test pdf.js |  | `95ef7cc7e6c80b3ce6e884407b0b695898a6f47a30b339d7472657406f041a82` |
| `polices/perf_text_cid_truetype.pdf` | 192 207 | suite de test pdf.js |  | `3cde3809d45249e086b2ad4b7efd950c9e59658024b7d0d68df2d8b0f413340f` |
| `polices/perf_text_cjk_nonembedded.pdf` | 7 521 | suite de test pdf.js |  | `347456be3aceb9131bceaec25ef8eef9565c32335bdc1f50f22d53b9aeaa2775` |
| `polices/perf_text_truetype.pdf` | 183 387 | suite de test pdf.js |  | `e572cbdc98fc8b8851b580ffaf32ca79b9f988f1794c37bd1e7cbced447303ee` |
| `polices/perf_text_type1.pdf` | 29 003 | suite de test pdf.js |  | `38d1dc69c92cba06044147822bd13a37109f73a8b8d46394b88b5192a4d6659d` |
| `polices/perf_text_type3_bitmap.pdf` | 30 131 | suite de test pdf.js |  | `a7bf301f67b6c70a3764eba2f49d6a035c8e91d80ce7700700721063ad5dfbe8` |
| `polices/perf_text_type3_vector.pdf` | 63 511 | suite de test pdf.js |  | `2ed808d7c9a39c51717be6734afb35f5ce20d7d23b9456ddd1a48ddcca7e739a` |
| `polices/simpletype3font.pdf` | 1 624 | suite de test pdf.js |  | `a5697cbc51c19816944658b91e3a41f86e98ae63fbd1007798994b073385aef1` |
| `polices/text_clip_cff_cid.pdf` | 7 393 | suite de test pdf.js |  | `81aaf48f55c659d29d7a533cd913bd9d028ffad183b46580a2122eebe5187506` |
| `polices/TrueType_without_cmap.pdf` | 3 989 | suite de test pdf.js |  | `3e111eef52ce9c0d0b1da043290df4b49a5670c89891fcb3db6962edb12e35aa` |
| `polices/Type3WordSpacing.pdf` | 2 693 | suite de test pdf.js |  | `8ac40dc49d40af8b5a50b442ced56b8db962c961a779a0284cd3a6207adc0bb7` |
| `polices/vertical.pdf` | 6 905 | suite de test pdf.js |  | `514511143db12309893fb69cdf98c76f2361c20da394f79fdff2c119ed7a4393` |
| `producteurs/bug1937438_af_from_latex.pdf` | 39 945 | suite de test pdf.js |  | `d0ed52c5298140bbe5bf9fb25a510667bf66befdcbc547c22c8a2652a26dfb36` |
| `producteurs/bug1937438_from_word.pdf` | 46 285 | suite de test pdf.js |  | `e03f741a8d0f6b6bbc811b39616f28d09ffb5098ab0f9ea89a0dfbd3f22e51b9` |
| `producteurs/form-empty-from-odt.pdf` | 54 848 | suite de test qpdf |  | `599304a4c47fa25155fc732cb9bffe2882ea39894e14f812905c4ec2f5826c14` |
| `producteurs/form-filled-by-acrobat.pdf` | 61 841 | suite de test qpdf |  | `465e30fe69d3b53d883ea1f01ea56a225ff0498f231ecda6f6fc986b9c5676ff` |
| `producteurs/franz.pdf` | 2 805 | suite de test pdf.js |  | `050ae194d21d9b257db62b85cdb632178bbcf71fd7e26c13e75e473a3fa4e8ea` |
| `producteurs/mixedfonts.pdf` | 405 432 | suite de test pdf.js |  | `04f17b84112d8130d75601f8f9c2027481a19df595aacb753ca3f9f3b08b2350` |
| `producteurs/openoffice.pdf` | 10 388 | suite de test pdf.js |  | `5c9841d641406b074f16087442a62f652057a607364b7aa82f8b4851f7e2acfc` |
| `producteurs/perf_text_office_nonembedded.pdf` | 10 927 | suite de test pdf.js |  | `9c489d794d5647a9e6b0cc41d83af727cc247211754fb6492de0730138361862` |
| `producteurs/TAMReview.pdf` | 674 629 | suite de test pdf.js |  | `a6db6d2f5e4f97e1e28a5919a875a40b89e38d0229b33cc023ca4d353f3ab3ec` |
| `producteurs/tracemonkey.pdf` | 1 016 315 | suite de test pdf.js |  | `3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113` |
| `signatures/signed_verified.pdf` | 10 252 | suite de test pdf.js |  | `1fe9051d9ac1f39828988040db471916251dc2953c1a32efd6596e4975da848c` |
| `structure/perf_structure_annotations.pdf` | 157 222 | suite de test pdf.js |  | `80e5e53fc043dd49bb8f18c581a51fe6c6f0753911e94c1cde8b97db540b1476` |
| `structure/perf_structure_forms.pdf` | 103 794 | suite de test pdf.js |  | `ca5755a52655abc11fbcc8c18bab9cbaea68f285fb9364145700565969b94bd2` |
| `structure/perf_structure_optional_content.pdf` | 347 801 | suite de test pdf.js |  | `89edde6d2acdee4fac1774cb17e457897a16f24e46e99b2fde9477952d202abb` |
| `structure/structure_no_parent_tree.pdf` | 2 394 | suite de test pdf.js |  | `8d19bf2392789721581a9bf2215e3aeb759a9ddad91338a58d7231fb9e62126e` |
| `structure/structure_simple.pdf` | 40 264 | suite de test pdf.js |  | `5b5510bb5f91c86aa0bbd4882842d4583c814e0e69a0cfbff0e70dc250a8d0f3` |
| `structure/tracemonkey_a11y.pdf` | 110 108 | suite de test pdf.js |  | `35730b8e3ad3873ccc70067a4688b08dde6240ff0d9db94e7f38dabeb6d748d7` |
