# Canaux et patron acteur

## L'idée en trois phrases

Quand une ressource ne supporte pas d'être utilisée par plusieurs threads (PDFium), la
solution la plus sûre n'est pas de la verrouiller partout mais de la confier à **un seul
thread** qui la possède : l'**acteur**. Les autres threads lui envoient des **messages** par un
**canal** et attendent la réponse. Les canaux de la bibliothèque standard
(`std::sync::mpsc`) transfèrent la **propriété** des messages d'un thread à l'autre : pas de
donnée partagée, donc pas de course possible.

## Dans ce projet

### 1. Deux canaux pour un appel — `crates/mega-core/src/service/mod.rs`, `ServiceHandle::call`

```rust
fn call<T>(&self, make: impl FnOnce(SyncSender<CoreResult<T>>) -> Request) -> CoreResult<T> {
    let (reply_tx, reply_rx) = mpsc::sync_channel(1);      // canal de réponse à une place
    self.tx.send(make(reply_tx)).map_err(|_| CoreError::ServiceStopped)?; // on envoie la requête…
    reply_rx.recv().map_err(|_| CoreError::ServiceStopped)? // …et on attend la réponse
}
```

- `self.tx` : l'émetteur du canal des requêtes (`mpsc::channel`, *multiple producers, single
  consumer*). Chaque handle en possède un clone ; l'acteur garde l'unique récepteur.
- `sync_channel(1)` : un canal borné à une place, créé **pour cette requête**. L'émetteur
  part dans le message ; le récepteur reste ici. C'est l'équivalent d'un « oneshot ».
- Si l'acteur est mort, `send` ou `recv` échoue : on le traduit en `ServiceStopped`.

### 2. Un message = une variante — `enum Request` (même fichier)

Chaque variante porte ses arguments **et** son `reply`. Le `match` de `Actor::handle` est
exhaustif : ajouter un message sans le traiter ne compile pas. C'est le contrat de l'acteur,
vérifié par le compilateur.

### 3. Le thread qui possède tout — `PdfService::spawn_with` et `Actor::run`

```rust
std::thread::Builder::new()
    .name("pdfium-actor".to_string())
    .spawn(move || Actor::new(cache_budget_bytes).run(rx))
```

`move` : la fermeture prend la propriété de `rx` (le récepteur). L'`Actor` (documents, cache,
file) est créé **dans** le thread et n'en sort jamais. Quand tous les émetteurs sont détruits,
`rx.recv()` renvoie `Err` et `run` se termine : le thread s'arrête sans signal explicite.

### 4. Lire en rafale pour prioriser — `Actor::run`

```rust
match rx.recv() { Ok(req) => self.queue.push(req.priority(), req), Err(_) => return }
loop { match rx.try_recv() { Ok(req) => self.queue.push(..), Err(_) => break } }
if let Some(req) = self.queue.pop() { self.handle(req); }
```

`recv()` bloque tant qu'il n'y a rien ; `try_recv()` ne bloque jamais. En vidant le canal
dans une file priorisée avant de traiter, une page visible arrivée après dix vignettes passe
devant elles. Test : `visible_requests_served_before_thumbnails`.

### 5. Côté Tauri : bloquer sans bloquer — `app/src-tauri/src/commands.rs`, `blocking`

Les commandes sont `async`, mais le handle **attend** (`recv`). On délègue à
`tauri::async_runtime::spawn_blocking`, un pool fait pour ça, puis on `.await`.

## Ce qui se passerait sans

- Verrou global (Lot 0) : correct mais aveugle : premier arrivé, premier servi, aucun rendu
  annulable, et l'interface attendait le verrou.
- `Arc<Mutex<Document>>` passé aux threads : le compilateur accepte, mais PDFium garde de
  l'état global entre appels (fiche `mutex-et-etat-partage`) : plantages vus en CI.
- Partager `rx` entre plusieurs acteurs : impossible, `Receiver` n'est pas `Sync` ; c'est le
  type qui impose « un seul consommateur ».

## Erreurs fréquentes

- Oublier `move` sur la fermeture du thread : `closure may outlive the current function`.
- Envoyer un type non `Send` (ex. un `Rc`) dans un canal : refusé à la compilation ; c'est
  pour cela que `Request` ne contient que des valeurs possédées (`String`, `Vec<u8>`, entiers).
- Garder un `MutexGuard` dans un message : interblocage probable ; on envoie des données, pas
  des verrous.
- Attendre une réponse **depuis** le thread de l'acteur (l'acteur s'envoie un message et
  attend) : interblocage garanti.

## Pour aller plus loin

- Rust Book, 16.2 (passage de messages) : https://doc.rust-lang.org/book/ch16-02-message-passing.html
- `std::sync::mpsc` : https://doc.rust-lang.org/std/sync/mpsc/index.html

## Exercice de lecture

`service/mod.rs`, `Request::priority` : toutes les opérations qui ne sont pas des rendus sont
traitées en priorité `Visible`. Imaginez une recherche sur un document de 2 000 pages lancée
pendant que l'utilisateur fait défiler : que voit-il ? Proposez un découpage de la recherche
qui garde la fluidité sans changer l'API du handle.
