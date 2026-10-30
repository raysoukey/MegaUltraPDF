// SPDX-License-Identifier: GPL-3.0-or-later
//! Tâches de développement, lancées avec `cargo xtask <commande>`.
//!
//! Le motif « xtask » est une convention de l'écosystème Rust : plutôt qu'un
//! Makefile ou des scripts shell, les tâches de dev sont un petit binaire Rust
//! du workspace, donc portables et testables.

#![forbid(unsafe_code)]

mod brand;
mod licenses;
mod pdfium;
mod stage;

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

const USAGE: &str = "\
Usage : cargo xtask <commande>

Commandes :
  fetch-pdfium [--target <triple>]   Télécharge la bibliothèque PDFium épinglée dans vendor/pdfium/
  gen-fixtures                       Régénère les PDF d'exemple de fixtures/ (sauf encrypted.pdf)
  stage-pdfium [--target <triple>]   Copie PDFium et sa licence dans app/src-tauri/resources/pdfium/
  licenses                           Inventaire des licences tierces (licenses.json, THIRD_PARTY_LICENSES.md)
  brand                              Icônes d'actions, logo et bannière à partir de assets/brand/
  fuzz [--iterations N] [--seed S]   Fichiers abîmés au hasard (graines S..S+N) : aucune panique
";

fn main() -> anyhow::Result<()> {
    // [Apprentissage] `std::env::args()` est un itérateur ; `skip(1)` saute le nom du
    // programme et `collect()` range le reste dans un `Vec<String>`.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fetch-pdfium") => fetch_pdfium(&args[1..]),
        Some("gen-fixtures") => gen_fixtures(),
        Some("stage-pdfium") => stage_pdfium(&args[1..]),
        Some("licenses") => licenses::generate(&workspace_root()),
        Some("brand") => brand::generate(&workspace_root()),
        Some("fuzz") => fuzz(&args[1..]),
        Some(other) => bail!("commande inconnue : {other}\n\n{USAGE}"),
        None => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

/// Racine du workspace : `xtask/` est un enfant direct de la racine.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ a toujours un parent")
        .to_path_buf()
}

/// Triple de la machine hôte, lu dans la sortie de `rustc -vV` (ligne `host:`).
fn host_target() -> anyhow::Result<String> {
    let out = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .context("impossible de lancer rustc")?;
    let text = String::from_utf8(out.stdout)?;
    text.lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(str::to_string)
        .context("ligne `host:` absente de la sortie de rustc -vV")
}

/// Cible demandée par `--target <triple>`, ou celle de la machine hôte.
fn target_arg(args: &[String], command: &str) -> anyhow::Result<String> {
    match args {
        [] => host_target(),
        [flag, value] if flag == "--target" => Ok(value.clone()),
        _ => bail!("arguments invalides pour {command}\n\n{USAGE}"),
    }
}

fn fetch_pdfium(args: &[String]) -> anyhow::Result<()> {
    pdfium::fetch(&workspace_root(), &target_arg(args, "fetch-pdfium")?)?;
    Ok(())
}

fn stage_pdfium(args: &[String]) -> anyhow::Result<()> {
    stage::stage(&workspace_root(), &target_arg(args, "stage-pdfium")?)?;
    Ok(())
}

/// Écrit `fixtures/simple.pdf` : 3 pages A4 titrées, générées par `mega-core`.
///
/// `fixtures/encrypted.pdf` et `fixtures/owner-only.pdf` ne sont pas régénérés ici : PDFium
/// ne sait pas chiffrer. Ils ont été produits une fois avec `qpdf --encrypt --user-password=secret
/// --owner-password=secret --bits=256 -- simple.pdf encrypted.pdf` et, pour la protection par
/// mot de passe propriétaire seul (s'ouvre sans mot de passe), `qpdf --encrypt "" secret 256 --
/// simple.pdf owner-only.pdf`.
fn gen_fixtures() -> anyhow::Result<()> {
    let dir = workspace_root().join("fixtures");
    std::fs::create_dir_all(&dir)?;
    let simple = dir.join("simple.pdf");
    std::fs::write(
        &simple,
        mega_core::testing::sample_pdf(3, "MegaUltraPDF — exemple"),
    )?;
    println!("écrit : {}", simple.display());
    let outlined = dir.join("outlined.pdf");
    std::fs::write(&outlined, mega_core::testing::outlined_pdf())?;
    println!("écrit : {}", outlined.display());
    let annotated = dir.join("annotated.pdf");
    std::fs::write(&annotated, mega_core::testing::annotated_pdf())?;
    println!("écrit : {}", annotated.display());
    let form = dir.join("form.pdf");
    std::fs::write(&form, mega_core::testing::form_pdf())?;
    println!("écrit : {}", form.display());
    let xfa = dir.join("xfa.pdf");
    std::fs::write(&xfa, mega_core::testing::xfa_pdf())?;
    println!("écrit : {}", xfa.display());
    // Page numérisée (image seule, aucun texte) pour l'OCR (Lot 8).
    let scanned = dir.join("scanned.pdf");
    std::fs::write(&scanned, mega_core::testing::scanned_pdf())?;
    println!("écrit : {}", scanned.display());
    let rotated = dir.join("rotated.pdf");
    std::fs::write(&rotated, mega_core::testing::rotated_pdf())?;
    println!("écrit : {}", rotated.display());
    // Texte dessiné dans le désordre : la sélection doit suivre l'ordre de lecture (0.12.2).
    let out_of_order = dir.join("out-of-order.pdf");
    std::fs::write(&out_of_order, mega_core::testing::out_of_order_pdf())?;
    println!("écrit : {}", out_of_order.display());
    let attachments = dir.join("attachments.pdf");
    std::fs::write(&attachments, mega_core::testing::attachments_pdf())?;
    println!("écrit : {}", attachments.display());
    // Caviardage d'une partie d'image (Lot 10).
    let mosaic = dir.join("image-mosaic.pdf");
    std::fs::write(&mosaic, mega_core::testing::image_mosaic_pdf())?;
    println!("écrit : {}", mosaic.display());
    // Déclaration PDF/A (Lot 13) : affichée dans les Propriétés, avertie à l'enregistrement.
    let pdfa = dir.join("pdfa.pdf");
    std::fs::write(&pdfa, mega_core::testing::pdfa_pdf())?;
    println!("écrit : {}", pdfa.display());
    gen_hostile_fixtures(&dir.join("robustesse"))?;
    gen_encrypted_fixtures(&dir)
}

/// Chiffrements que l'audit du Lot 13 a trouvés non testés, produits par qpdf à partir de
/// `simple.pdf` dans `fixtures/chiffrement/` (mot de passe utilisateur `u`, propriétaire `o`,
/// sauf le mot de passe Unicode). qpdf tire des clés au hasard : les fichiers changent à chaque
/// régénération, on ne les régénère donc que s'ils manquent.
fn gen_encrypted_fixtures(fixtures: &Path) -> anyhow::Result<()> {
    let dir = fixtures.join("chiffrement");
    std::fs::create_dir_all(&dir)?;
    let simple = fixtures.join("simple.pdf");
    let cases: [(&str, &[&str]); 4] = [
        (
            "rc4-40.pdf",
            &[
                "--allow-weak-crypto",
                "--encrypt",
                "--user-password=u",
                "--owner-password=o",
                "--bits=40",
                "--",
            ],
        ),
        (
            "aes128.pdf",
            &[
                "--encrypt",
                "--user-password=u",
                "--owner-password=o",
                "--bits=128",
                "--use-aes=y",
                "--",
            ],
        ),
        (
            "aes256-r5.pdf",
            &[
                "--encrypt",
                "--user-password=u",
                "--owner-password=o",
                "--bits=256",
                "--force-R5",
                "--",
            ],
        ),
        // Mot de passe accentué et chinois : R6 le passe par SASLprep puis UTF-8 (§7.6.4.3.3).
        (
            "mot-de-passe-unicode.pdf",
            &[
                "--encrypt",
                "--user-password=été密码",
                "--owner-password=o",
                "--bits=256",
                "--",
            ],
        ),
    ];
    for (name, args) in cases {
        let out = dir.join(name);
        if out.exists() {
            continue;
        }
        match std::process::Command::new(qpdf_exe())
            .args(args)
            .arg(&simple)
            .arg(&out)
            .status()
        {
            Ok(status) if status.success() => println!("écrit : {}", out.display()),
            _ => println!("qpdf introuvable : {} non produit", out.display()),
        }
    }
    let pubsec = dir.join("certificat.pdf");
    std::fs::write(&pubsec, mega_core::testing::hostile::pubsec_pdf())?;
    println!("écrit : {}", pubsec.display());
    Ok(())
}

/// Campagne de fuzzing (Lot 13) : `iterations` variantes abîmées des fixtures, graines
/// `seed..seed + iterations`. La graine du cas en cours est écrite dans
/// `target/fuzz-graine.txt` **avant** son lancement : si le processus s'arrête net (plantage
/// natif de PDFium), on sait quel cas rejouer (`--seed G --iterations 1`).
fn fuzz(args: &[String]) -> anyhow::Result<()> {
    let mut iterations: u64 = 1000;
    let mut seed: u64 = 0;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().context("valeur manquante après l'option")?;
        match flag.as_str() {
            "--iterations" => iterations = value.parse()?,
            "--seed" => seed = value.parse()?,
            other => bail!("option inconnue pour fuzz : {other}\n\n{USAGE}"),
        }
    }
    let root = workspace_root();
    // Les fichiers lents par construction (bombe, 5 000 formulaires) sont écartés.
    let mut originals = Vec::new();
    for dir in [
        root.join("fixtures"),
        root.join("fixtures").join("robustesse"),
    ] {
        let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
            .filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                !matches!(
                    name.as_deref(),
                    Some("bombe-flate.pdf" | "xobjects-profonds.pdf")
                )
            })
            .collect();
        names.sort();
        for path in names {
            originals.push(std::fs::read(&path)?);
        }
    }
    anyhow::ensure!(!originals.is_empty(), "aucune fixture PDF trouvée");
    let marker = root.join("target").join("fuzz-graine.txt");
    std::fs::create_dir_all(root.join("target"))?;
    let start = std::time::Instant::now();
    let mut opened = 0u64;
    for s in seed..seed.saturating_add(iterations) {
        std::fs::write(&marker, s.to_string())?;
        let original = &originals[(s % originals.len() as u64) as usize];
        let bytes = mega_core::testing::fuzz::mutate(original, s);
        opened += u64::from(mega_core::testing::fuzz::exercise(bytes));
        if (s - seed + 1).is_multiple_of(1000) {
            println!("{} cas…", s - seed + 1);
        }
    }
    println!(
        "{iterations} cas (graines {seed}..{}), {opened} fichiers ouverts, aucune panique, {:.0} s",
        seed.saturating_add(iterations),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Écrit le corpus de PDF malformés ou hostiles (Lot 13) dans `fixtures/robustesse/`, et, si
/// qpdf est installé, une version **linéarisée** de `simple.pdf` (« affichage web rapide »).
fn gen_hostile_fixtures(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, bytes) in mega_core::testing::hostile::corpus() {
        let path = dir.join(name);
        std::fs::write(&path, bytes)?;
        println!("écrit : {}", path.display());
    }
    let linearized = dir.join("linearise.pdf");
    let simple = dir
        .parent()
        .map(|d| d.join("simple.pdf"))
        .unwrap_or_default();
    match std::process::Command::new(qpdf_exe())
        .arg("--linearize")
        .arg("--deterministic-id")
        .arg(&simple)
        .arg(&linearized)
        .status()
    {
        Ok(status) if status.success() => println!("écrit : {}", linearized.display()),
        _ => println!("qpdf introuvable : {} non régénéré", linearized.display()),
    }
    Ok(())
}

/// L'exécutable qpdf : `MEGA_QPDF` s'il est défini, sinon `qpdf` dans le `PATH`.
fn qpdf_exe() -> PathBuf {
    std::env::var_os("MEGA_QPDF").map_or_else(|| PathBuf::from("qpdf"), PathBuf::from)
}
