// SPDX-License-Identifier: GPL-3.0-or-later
//! Nos fichiers s'ouvrent-ils **ailleurs** ? (Lot 13, audit C1 ; étendu au Lot 14, audit E1)
//!
//! Chaque édition est appliquée à un vrai fichier, le résultat est enregistré, puis contrôlé
//! par `qpdf --check`, un validateur indépendant de PDFium et de lopdf : syntaxe, table de
//! références, flux. Un avertissement compte comme un échec : un autre lecteur pourrait ne
//! pas réparer ce que qpdf signale.
//!
//! qpdf est un outil externe : sans lui, ces tests affichent un avertissement et passent, sauf
//! si la variable `MEGA_REQUIRE_QPDF=1` est posée (la CI la pose). L'exécutable est pris dans
//! `MEGA_QPDF`, sinon dans le `PATH`. Deux relecteurs **facultatifs** s'ajoutent à qpdf quand
//! leur variable est posée : MuPDF (`MEGA_MUTOOL`) et pdf.js (`MEGA_PDFJS_CHECK`, script lancé
//! par `node`). Le corpus de fichiers réels (`MEGA_CORPUS`) et veraPDF (`MEGA_VERAPDF`) sont
//! facultatifs aussi : voir `CONTRIBUTING.md`.
//!
//! Les commandes sont écrites en JSON, exactement comme l'interface les envoie
//! (`app/src/lib/api.ts`) : le test couvre aussi leur forme sérialisée.
//!
//! Organisation (un seul binaire de test, `cargo test --test conformance`) :
//! - `edits` : chaque variante d'`EditCommand`, et la preuve qu'aucune n'est oubliée ;
//! - `encrypted` : fichiers chiffrés par qpdf, dont le XMP chiffré survit aux éditions ;
//! - `catalogue` : les 17 structures du catalogue survivent aux éditions, à la fusion, à
//!   l'extraction et à la scission ;
//! - `pages` : fusion, scission, extraction, déplacements dans l'arbre des pages ;
//! - `signature` : signatures numériques, visibles ou non, sur des structures inhabituelles ;
//! - `corpus` : fichiers abîmés, corpus de PDF réels, veraPDF.
//!
//! \[Apprentissage\] **Un test d'intégration en plusieurs fichiers.** Cargo compile chaque
//! fichier `tests/*.rs` en un binaire séparé, et aussi chaque dossier `tests/<nom>/main.rs` :
//! ce fichier est la racine du binaire `conformance`, et ses `mod edits;` vont chercher
//! `tests/conformance/edits.rs`, etc. Les fichiers d'un tel dossier ne deviennent **pas** des
//! binaires à leur tour (seuls `main.rs` et les fichiers posés directement dans `tests/` le
//! sont) : les aides partagées ci-dessous sont écrites une fois et visibles des modules par
//! `use super::…` (ou `crate::…`, puisque `main.rs` est la racine du binaire).

mod catalogue;
mod corpus;
mod edits;
mod encrypted;
mod pages;
mod signature;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mega_core::Document;
use mega_core::edit::EditCommand;
use serde_json::Value;

/// Avertissement affiché quand qpdf manque (audit E1 : sans lui, les tests se sautaient en
/// silence et `cargo test` restait vert sans aucun contrôle).
const QPDF_MISSING: &str =
    "AVERTISSEMENT : qpdf introuvable, contrôle de conformité SAUTÉ (poser MEGA_QPDF)";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../../fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Le chemin donné par la variable `name`, si elle est posée et non vide.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// L'exécutable qpdf, s'il répond ; sans lui, panique seulement si la CI l'exige, sinon
/// affiche [`QPDF_MISSING`]. Appelé **une fois en tête** de chaque test qui s'en sert : un
/// avertissement par test sauté.
fn qpdf() -> Option<PathBuf> {
    let exe = env_path("MEGA_QPDF").unwrap_or_else(|| PathBuf::from("qpdf"));
    let works = Command::new(&exe)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if works {
        return Some(exe);
    }
    assert!(
        std::env::var("MEGA_REQUIRE_QPDF").as_deref() != Ok("1"),
        "qpdf introuvable alors que MEGA_REQUIRE_QPDF=1"
    );
    // Pas `eprintln!` : le harnais de test capture ce que les macros `print!` et `eprint!`
    // écrivent et ne l'affiche que pour un test qui échoue ; or un test sauté **passe**.
    // Écrire directement sur la sortie d'erreur du processus contourne la capture : le
    // message s'affiche dans un `cargo test` ordinaire. L'échec d'écriture est sans
    // conséquence (le test passe de toute façon), d'où le `let _`.
    let _ = writeln!(std::io::stderr(), "{QPDF_MISSING}");
    None
}

/// Les deux sorties d'un programme, mises bout à bout (pour les messages d'échec).
fn output_text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Délai accordé à un outil externe (qpdf, MuPDF, pdf.js, veraPDF) pour un fichier.
const TOOL_LIMIT: Duration = Duration::from_secs(60);

/// Délai laissé aux fils de lecture pour rendre les sorties une fois le programme terminé (ou
/// tué) : au-delà, un petit-enfant garde le tuyau ouvert, et l'on renonce.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Lance `cmd` et attend sa fin au plus `limit` ; au-delà, le processus est tué. `None` si le
/// programme n'a pas pu être lancé, s'il a dépassé le délai, ou si ses sorties ne se ferment
/// pas : l'appelant en fait un échec.
///
/// Pourquoi : un outil bloqué (fichier qui le fait boucler, `node` qui attend une entrée)
/// figerait toute la suite de tests sans rien dire. `Command::output` n'a pas de délai.
///
/// \[Apprentissage\] **Deux fils de lecture et un canal.** Un programme qui écrit beaucoup
/// remplit le tuyau (*pipe*) de sa sortie, puis s'arrête en attendant qu'on le vide : si l'on
/// se contentait d'attendre sa fin, les deux processus s'attendraient l'un l'autre. Chaque
/// sortie est donc vidée par son propre fil (`std::thread::spawn`) pendant que le fil
/// principal surveille l'horloge avec `try_wait` (qui ne bloque pas). Les fils envoient ce
/// qu'ils ont lu par un canal (`std::sync::mpsc`) ; le fil principal le reçoit avec
/// `recv_timeout`, qui abandonne après un délai. Attendre les fils par `join` serait risqué :
/// tuer le programme ne ferme pas un tuyau qu'un petit-enfant (`node`, l'enveloppe d'une JVM)
/// garde ouvert, et `join` attendrait pour toujours. Un fil abandonné finira seul quand le
/// tuyau se fermera.
fn run_with_timeout(cmd: &mut Command, limit: Duration) -> Option<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    // `true` : sortie standard ; `false` : sortie d'erreur.
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    let sender_err = sender.clone();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send((true, bytes));
    });
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        let _ = sender_err.send((false, bytes));
    });
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            // Délai dépassé (ou état illisible) : on tue, puis on attend la fin pour ne pas
            // laisser de processus zombie.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let drain_until = Instant::now() + DRAIN_GRACE;
    let (mut out, mut err) = (None, None);
    while out.is_none() || err.is_none() {
        let left = drain_until.saturating_duration_since(Instant::now());
        // Délai écoulé ou fils disparus : sorties incomplètes, donc échec.
        let (is_stdout, bytes) = receiver.recv_timeout(left).ok()?;
        if is_stdout {
            out = Some(bytes);
        } else {
            err = Some(bytes);
        }
    }
    Some(Output {
        status: status?,
        stdout: out?,
        stderr: err?,
    })
}

/// `qpdf --check` sur `path` (avec `password` s'il est chiffré) : sans erreur **ni**
/// avertissement. Puis, s'ils sont fournis, MuPDF et pdf.js relisent le même fichier
/// ([`assert_cross_readers_accept`]).
fn assert_conforms(qpdf: &Path, path: &Path, password: Option<&str>, what: &str) {
    assert_qpdf_accepts(qpdf, path, password, what);
    assert_cross_readers_accept(path, password, what);
}

fn assert_qpdf_accepts(qpdf: &Path, path: &Path, password: Option<&str>, what: &str) {
    if let Err(report) = qpdf_verdict(qpdf, path, password) {
        panic!("{what} : qpdf --check a échoué {report}");
    }
}

/// Lance qpdf sur `path` avec `args` (et `--password=…` si `password` est donné), sous le
/// délai [`TOOL_LIMIT`]. Seul endroit qui invoque qpdf sur un fichier.
fn run_qpdf(qpdf: &Path, path: &Path, password: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(qpdf);
    if let Some(p) = password {
        cmd.arg(format!("--password={p}"));
    }
    cmd.args(args).arg(path);
    run_with_timeout(&mut cmd, TOOL_LIMIT)
        .unwrap_or_else(|| panic!("qpdf injoignable ou bloqué plus de 60 s sur {path:?}"))
}

/// Le seul `qpdf --check`, sans paniquer : `Err` porte le code de sortie et le rapport. Le
/// corpus s'en sert pour lister tous ses échecs d'un coup, sans les relecteurs facultatifs
/// (un fichier réel que pdf.js lisait déjà mal ne dirait rien de notre écriture).
fn qpdf_verdict(qpdf: &Path, path: &Path, password: Option<&str>) -> Result<(), String> {
    let out = run_qpdf(qpdf, path, password, &["--check"]);
    let report = output_text(&out);
    if out.status.code() == Some(0) && !report.contains("WARNING") {
        Ok(())
    } else {
        Err(format!("({:?})\n{report}", out.status.code()))
    }
}

/// MuPDF, si `MEGA_MUTOOL` désigne `mutool` (sinon : sauté).
fn mutool() -> Option<PathBuf> {
    env_path("MEGA_MUTOOL")
}

/// Le script de contrôle pdf.js (`scripts/audit/pdfjs-check.mjs`), si `MEGA_PDFJS_CHECK`
/// le désigne (sinon : sauté). Il est lancé par `node`.
fn pdfjs_check() -> Option<PathBuf> {
    env_path("MEGA_PDFJS_CHECK")
}

/// Relecture facultative par deux autres moteurs que PDFium et qpdf : MuPDF et pdf.js,
/// chacun sauté si sa variable n'est pas posée.
fn assert_cross_readers_accept(path: &Path, password: Option<&str>, what: &str) {
    if let Some(mutool) = mutool() {
        assert_mutool_reads(&mutool, path, password, what);
    }
    if let Some(script) = pdfjs_check() {
        assert_pdfjs_reads(&script, path, password, what);
    }
}

/// `mutool info` sur `path`, sous le délai [`TOOL_LIMIT`].
fn mutool_info(mutool: &Path, path: &Path, password: Option<&str>, what: &str) -> Output {
    let mut cmd = Command::new(mutool);
    cmd.arg("info");
    if let Some(p) = password {
        cmd.args(["-p", p]);
    }
    cmd.arg(path);
    run_with_timeout(&mut cmd, TOOL_LIMIT)
        .unwrap_or_else(|| panic!("{what} : mutool injoignable ou bloqué plus de 60 s"))
}

/// `mutool info` répond 0 et ne signale aucune « error ». Un outil désigné mais injoignable
/// fait échouer le test : on l'a demandé.
fn assert_mutool_reads(mutool: &Path, path: &Path, password: Option<&str>, what: &str) {
    let out = mutool_info(mutool, path, password, what);
    // Les diagnostics de MuPDF vont sur la sortie d'erreur ; la sortie standard recopie le
    // dictionnaire `/Info`, dont le texte nous appartient et pourrait contenir le mot.
    let diagnostics = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(
        out.status.success() && !diagnostics.contains("error"),
        "{what} : mutool info a échoué ({:?})\n{}",
        out.status.code(),
        output_text(&out)
    );
}

/// Le script pdf.js (lancé par `node`) imprime `"ok":true` pour le fichier.
fn assert_pdfjs_reads(script: &Path, path: &Path, password: Option<&str>, what: &str) {
    let mut cmd = Command::new("node");
    cmd.arg(script);
    if let Some(p) = password {
        cmd.args(["--password", p]);
    }
    cmd.arg(path);
    let out = run_with_timeout(&mut cmd, TOOL_LIMIT)
        .unwrap_or_else(|| panic!("{what} : node injoignable ou bloqué plus de 60 s (pdf.js)"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Le script imprime un objet JSON par fichier ; d'autres lignes (avertissements de
    // pdf.js) peuvent le précéder.
    let verdict: Option<Value> = stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .find_map(|l| serde_json::from_str(l).ok());
    assert!(
        verdict.is_some_and(|v| v["ok"] == Value::Bool(true)),
        "{what} : pdf.js n'ouvre pas le fichier\n{}",
        output_text(&out)
    );
}

/// Un cas de contrôle : un fichier source (ouvert avec `password`, s'il est chiffré) et les
/// commandes JSON à lui appliquer, dans l'ordre.
struct Case {
    what: &'static str,
    source: PathBuf,
    password: Option<&'static str>,
    commands: Vec<Value>,
    /// Texte que la page 1 du fichier produit doit livrer (son texte, ou le contenu d'une de
    /// ses annotations) : un texte hors ASCII ajouté doit se relire, pas seulement passer qpdf.
    expect_text: Option<&'static str>,
}

impl Case {
    /// Un cas sur une fixture du dépôt, sans mot de passe.
    fn new(what: &'static str, source: &str, commands: Vec<Value>) -> Case {
        Case {
            what,
            source: fixture(source),
            password: None,
            commands,
            expect_text: None,
        }
    }

    /// Le même cas, dont la sortie doit se relire avec `text` sur sa page 1.
    fn expecting(self, text: &'static str) -> Case {
        Case {
            expect_text: Some(text),
            ..self
        }
    }
}

/// Le mot de passe d'ouverture du fichier **produit** : celui de la source, remplacé par
/// celui d'une commande `protect`, retiré par `unprotect`.
fn output_password(case: &Case) -> Option<String> {
    let mut password = case.password.map(str::to_string);
    for command in &case.commands {
        match command["kind"].as_str() {
            Some("protect") => {
                password = command["protection"]["user_password"]
                    .as_str()
                    .filter(|p| !p.is_empty())
                    .map(str::to_string);
            }
            Some("unprotect") => password = None,
            _ => {}
        }
    }
    password
}

/// Applique les commandes du cas à sa source, enregistre, contrôle avec qpdf (et les
/// relecteurs facultatifs). Renvoie les octets écrits, pour les contrôles propres au cas.
fn check(qpdf: &Path, case: &Case) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let out = apply_and_save(case, dir.path());
    let password = output_password(case);
    assert_conforms(qpdf, &out, password.as_deref(), case.what);
    if let Some(text) = case.expect_text {
        assert_text_reads_back(&out, password.as_deref(), text, case.what);
    }
    std::fs::read(&out).unwrap()
}

/// La page 1 du fichier `path` livre `text` : dans son texte, ou, une fois aplatie, dans ce
/// que dessinent ses annotations (un tampon dessine son texte dans son apparence, hors du
/// texte de la page). Le `/Contents` d'une annotation ne compte pas : c'est la chaîne que la
/// commande a fournie, elle prouverait le stockage, pas le dessin.
fn assert_text_reads_back(path: &Path, password: Option<&str>, text: &str, what: &str) {
    let mut doc = Document::open(path, password).unwrap();
    let page = doc.page_text(0).unwrap();
    if page.contains(text) {
        return;
    }
    doc.flatten_all().unwrap();
    let flattened = doc.page_text(0).unwrap();
    assert!(
        flattened.contains(text),
        "{what} : « {text} » ne se relit pas (texte de la page : {page:?}, aplatie : \
         {flattened:?})"
    );
}

/// Ouvre la source du cas, applique ses commandes par `Document::apply` (comme l'interface),
/// enregistre par `save_as` dans `dir` et renvoie le chemin écrit.
fn apply_and_save(case: &Case, dir: &Path) -> PathBuf {
    let what = case.what;
    let mut doc = Document::open(&case.source, case.password)
        .unwrap_or_else(|e| panic!("{what} : source illisible : {e}"));
    for json in &case.commands {
        let command = parse(json, what);
        doc.apply(&command)
            .unwrap_or_else(|e| panic!("{what} : {json} refusée : {e}"));
    }
    let out = dir.join("sortie.pdf");
    doc.save_as(&out)
        .unwrap_or_else(|e| panic!("{what} : enregistrement refusé : {e}"));
    out
}

/// La commande que décrit `json`, telle que l'interface l'enverrait.
fn parse(json: &Value, what: &str) -> EditCommand {
    serde_json::from_value(json.clone())
        .unwrap_or_else(|e| panic!("{what} : commande {json} illisible : {e}"))
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Value {
    serde_json::json!({ "x": x, "y": y, "w": w, "h": h })
}

fn style() -> Value {
    serde_json::json!({
        "color": { "r": 200, "g": 0, "b": 0 },
        "fill": null,
        "opacity": 1.0,
        "stroke_width": 2.0,
        "font_size": 12.0
    })
}

fn annotation(kind: &str, geometry: Value) -> Value {
    annotation_with(kind, geometry, "Contrôle de conformité")
}

/// Une annotation `kind` sur la page 1, avec le texte `contents` (celui d'un tampon est
/// dessiné dans son apparence).
fn annotation_with(kind: &str, geometry: Value, contents: &str) -> Value {
    serde_json::json!({
        "kind": "add_annotation",
        "page": 0,
        "annotation": {
            "kind": kind,
            "geometry": geometry,
            "style": style(),
            "contents": contents,
            "author": "Moi"
        }
    })
}

/// Toutes les variantes d'`EditCommand`, sous leur nom JSON (`kind`). Tenue à jour avec
/// [`variant`] : une variante ajoutée à l'enum fait échouer la compilation de `variant`, qui
/// renvoie ici (CLAUDE.md : « toute nouvelle variante d'`EditCommand` y entre »).
const ALL_VARIANTS: [&str; 29] = [
    "rotate_pages",
    "delete_pages",
    "move_pages",
    "insert_blank_page",
    "insert_pages_from_file",
    "add_annotation",
    "delete_annotation",
    "update_annotation",
    "move_annotation",
    "set_field_value",
    "flatten_form",
    "insert_signature",
    "delete_object",
    "move_object",
    "resize_object",
    "set_object_text",
    "insert_text",
    "insert_image",
    "redact",
    "watermark",
    "page_numbers",
    "crop_pages",
    "set_metadata",
    "protect",
    "unprotect",
    "optimize",
    "add_attachment",
    "delete_attachment",
    "ocr_page",
];

/// Le nom JSON de la variante de `command`.
///
/// \[Apprentissage\] **`match` exhaustif comme garde-fou.** Sans bras `_ =>`, le compilateur
/// exige un bras par variante : ajouter une variante à `EditCommand` casse la compilation de
/// ce test tant qu'on ne l'a pas nommée ici, et donc tant qu'on n'a pas pensé à la contrôler.
fn variant(command: &EditCommand) -> &'static str {
    match command {
        EditCommand::RotatePages { .. } => "rotate_pages",
        EditCommand::DeletePages { .. } => "delete_pages",
        EditCommand::MovePages { .. } => "move_pages",
        EditCommand::InsertBlankPage { .. } => "insert_blank_page",
        EditCommand::InsertPagesFromFile { .. } => "insert_pages_from_file",
        EditCommand::AddAnnotation { .. } => "add_annotation",
        EditCommand::DeleteAnnotation { .. } => "delete_annotation",
        EditCommand::UpdateAnnotation { .. } => "update_annotation",
        EditCommand::MoveAnnotation { .. } => "move_annotation",
        EditCommand::SetFieldValue { .. } => "set_field_value",
        EditCommand::FlattenForm => "flatten_form",
        EditCommand::InsertSignature { .. } => "insert_signature",
        EditCommand::DeleteObject { .. } => "delete_object",
        EditCommand::MoveObject { .. } => "move_object",
        EditCommand::ResizeObject { .. } => "resize_object",
        EditCommand::SetObjectText { .. } => "set_object_text",
        EditCommand::InsertText { .. } => "insert_text",
        EditCommand::InsertImage { .. } => "insert_image",
        EditCommand::Redact { .. } => "redact",
        EditCommand::Watermark { .. } => "watermark",
        EditCommand::PageNumbers { .. } => "page_numbers",
        EditCommand::CropPages { .. } => "crop_pages",
        EditCommand::SetMetadata { .. } => "set_metadata",
        EditCommand::Protect { .. } => "protect",
        EditCommand::Unprotect { .. } => "unprotect",
        EditCommand::Optimize { .. } => "optimize",
        EditCommand::AddAttachment { .. } => "add_attachment",
        EditCommand::DeleteAttachment { .. } => "delete_attachment",
        EditCommand::OcrPage { .. } => "ocr_page",
    }
}

/// Les variantes que doit couvrir une liste de cas : toutes, sauf `ocr_page`, qui lance
/// Tesseract (absent de la CI). Ce qu'elle écrit dans le fichier, la couche de mots
/// invisibles, est contrôlé directement (`edits::ocr_text_layers_produce_conforming_files`).
fn expected_variants() -> Vec<&'static str> {
    ALL_VARIANTS
        .iter()
        .copied()
        .filter(|v| *v != "ocr_page")
        .collect()
}

/// Les variantes d'`expected_variants` qu'aucune commande de `commands` n'exerce.
fn missing_variants<'a>(commands: impl IntoIterator<Item = &'a Value>) -> Vec<&'static str> {
    let covered: Vec<&'static str> = commands
        .into_iter()
        .map(|json| variant(&parse(json, "couverture")))
        .collect();
    expected_variants()
        .into_iter()
        .filter(|v| !covered.contains(v))
        .collect()
}
