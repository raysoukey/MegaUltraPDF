# `Drop` et ressources natives (RAII)

## L'idée en trois phrases

Rust n'a pas de ramasse-miettes : quand une valeur sort de portée, son destructeur `Drop`
s'exécute **immédiatement et une seule fois**, dans un ordre connu. Une ressource qui n'est
pas de la mémoire (fichier, handle PDFium, verrou) se range donc dans une valeur dont le
`Drop` la libère : c'est le principe RAII (*Resource Acquisition Is Initialization*). On ne
peut pas oublier de libérer, ni libérer deux fois, parce que ce n'est pas nous qui le faisons.

## Dans ce projet

### 1. Le document PDFium — `crates/mega-core/src/document.rs`, `impl Drop for Document`

```rust
impl Drop for Document {
    fn drop(&mut self) {
        let _guard = engine::guard();   // PDFium n'aime pas être appelée de deux threads
        self.inner.take();              // libère le PdfDocument (FPDF_CloseDocument) sous le jeton
    }
}
```

- `inner` est un `Option<PdfDocument>` justement pour pouvoir le **retirer** (`take`) dans
  `drop` tout en tenant le jeton. Le `PdfDocument` de pdfium-render a lui-même un `Drop`
  qui appelle `FPDF_CloseDocument` ; le nôtre ne fait qu'imposer **quand** et **sous quel
  verrou** cela se passe.
- L'environnement de formulaire (Lot 4) vit dans ce `PdfDocument` : pdfium-render le crée à
  l'ouverture (`FPDFDOC_InitFormFillEnvironment`) et le détruit dans son `Drop`. Nous ne
  gérons ni son ouverture ni sa fermeture : lire `forms/mod.rs`, aucune ligne ne s'en occupe.

### 2. Le jeton PDFium — `crates/mega-core/src/engine.rs`, `guard()`

```rust
pub(crate) fn guard() -> PdfiumGuard { … }   // un MutexGuard<'static, ()>
```

Un `MutexGuard` libère le verrou dans son `Drop`. C'est pourquoi on écrit
`let _guard = engine::guard();` et jamais `let _ = engine::guard();` : la seconde forme
**jette** la valeur immédiatement, donc déverrouille sur-le-champ (fiche
[`mutex-et-etat-partage`](mutex-et-etat-partage.md)). Dans `set_field_value`
(`forms/mod.rs`), le bloc `{ let _guard = …; … }` est fermé **avant** `reload_from_bytes`,
qui reprend le jeton : sans ce bloc, le second `guard()` bloquerait pour toujours (le verrou
n'est pas réentrant).

### 3. Le fichier temporaire — `crates/mega-core/src/edit.rs`, `write_atomically`

`tempfile::NamedTempFile` supprime le fichier dans son `Drop`. Si `persist` échoue, l'erreur
contient le temporaire, qui est détruit avec elle : pas de fichier orphelin. Test
`save_as_to_directory_path_leaves_original_intact_and_no_temp_file`.

### 4. Les objets de page — `crates/mega-core/src/forms/signature.rs`

`PdfPagePathObject::new(...)` crée un objet natif **non attaché** ; `add_path_object(obj)` le
donne à la page (la page en devient propriétaire). Si une erreur survient entre les deux,
l'objet est détruit par son `Drop` (`FPDFPageObj_Destroy`) : rien ne fuit.

## Ce qui se passerait sans

En C, chaque chemin d'erreur devrait appeler `FPDF_CloseDocument`, `FPDFPageObj_Destroy`,
`remove(tmpfile)` à la main ; un `return` oublié = fuite, un double appel = plantage. Ici le
compilateur insère les appels, y compris sur les sorties par `?`.

## Erreurs fréquentes

- `let _ = x;` au lieu de `let _x = x;` : la ressource est libérée tout de suite.
- Ordre de destruction : les variables locales sont détruites en **ordre inverse** de leur
  déclaration ; un guard déclaré *après* la ressource qu'il protège est libéré *avant* elle.
  C'est le piège résolu par `inner: Option<…>` + `take()` dans `Drop for Document`.
- Un `Drop` ne peut pas retourner d'erreur : on y fait le minimum (libérer), jamais une
  écriture qui pourrait échouer.

## Pour aller plus loin

Rust Book, chapitre 15.3 « Running Code on Cleanup with the Drop Trait » :
<https://doc.rust-lang.org/book/ch15-03-drop.html>.

## Exercice de lecture

`forms/mod.rs`, `flatten_all` : pourquoi le `for` qui aplatit les pages est-il enfermé dans un
bloc `{ … }` avant `self.reload_from_bytes(...)` ? Que se passerait-il si l'on retirait les
accolades ?
