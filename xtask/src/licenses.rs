// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask licenses` : inventaire des composants tiers (crates Rust et paquets npm).
//!
//! Lit `cargo metadata` et `pnpm licenses list --json`, puis écrit
//! `app/src/lib/generated/licenses.json` (lu par l'onglet « À propos ») et
//! `THIRD_PARTY_LICENSES.md` à la racine : tableau des composants **et** textes complets de
//! leurs licences (les licences MIT, BSD… exigent que la notice accompagne le binaire). Les
//! deux fichiers sont commités et regénérés à chaque version.
//!
//! \[Apprentissage\] `serde_json` sait désérialiser une **partie** d'un JSON : les structures
//! ci-dessous ne déclarent que les champs utiles, les autres (des dizaines dans la sortie de
//! `cargo metadata`) sont ignorés sans erreur.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// D'où vient un composant tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Crate Rust (registre crates.io ou dépôt git).
    Crate,
    /// Paquet npm (interface).
    Npm,
}

/// Une ligne de l'inventaire : un composant, sa version et sa licence déclarée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Nom du paquet (crate ou paquet npm).
    pub name: String,
    /// Version résolue (une entrée par version installée).
    pub version: String,
    /// Expression SPDX déclarée ; `None` quand le paquet ne fournit qu'un fichier de licence.
    pub license: Option<String>,
    /// Écosystème d'origine.
    pub source: Source,
    /// Dossier du paquet sur cette machine, où chercher ses fichiers de licence (non publié
    /// dans `licenses.json` : c'est un chemin local).
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

/// Ce qu'on lit de `cargo metadata --format-version 1`.
#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    #[serde(default)]
    manifest_path: String,
}

/// Ce qu'on lit d'une entrée de `pnpm licenses list --json` (regroupée par licence).
#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    versions: Vec<String>,
    license: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
}

/// Les crates tierces d'une sortie `cargo metadata` : les membres du workspace (nos propres
/// crates) sont exclus. Toutes les plateformes et les dépendances de test sont comprises :
/// l'inventaire est un sur-ensemble, jamais un oubli.
pub fn crate_entries_from_metadata(json: &str) -> anyhow::Result<Vec<Entry>> {
    let meta: Metadata =
        serde_json::from_str(json).context("sortie de cargo metadata illisible")?;
    let members: HashSet<&str> = meta.workspace_members.iter().map(String::as_str).collect();
    Ok(meta
        .packages
        .into_iter()
        .filter(|p| !members.contains(p.id.as_str()))
        .map(|p| Entry {
            dir: Path::new(&p.manifest_path).parent().map(Path::to_path_buf),
            name: p.name,
            version: p.version,
            license: p.license,
            source: Source::Crate,
        })
        .collect())
}

/// Les paquets d'une sortie `pnpm licenses list --json`, une entrée par version installée.
pub fn npm_entries(json: &str) -> anyhow::Result<Vec<Entry>> {
    let groups: BTreeMap<String, Vec<NpmPackage>> =
        serde_json::from_str(json).context("sortie de pnpm licenses illisible")?;
    let mut entries = Vec::new();
    for (group_license, packages) in groups {
        for p in packages {
            // La licence du groupe sert de repli si l'entrée n'en déclare pas.
            let license = p.license.or_else(|| Some(group_license.clone()));
            for (i, version) in p.versions.into_iter().enumerate() {
                entries.push(Entry {
                    name: p.name.clone(),
                    version,
                    license: license.clone(),
                    source: Source::Npm,
                    // pnpm donne un chemin par version installée, dans le même ordre.
                    dir: p.paths.get(i).map(PathBuf::from),
                });
            }
        }
    }
    Ok(entries)
}

/// Trie par nom (sans tenir compte de la casse) puis par version, et retire les doublons
/// exacts (même source, même nom, même version).
pub fn sorted_unique(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.version.cmp(&b.version))
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut seen = HashSet::new();
    entries.retain(|e| seen.insert((e.source, e.name.clone(), e.version.clone())));
    entries
}

/// Une cellule de tableau Markdown : `|` échappé (une expression de licence npm peut en
/// contenir et casserait la ligne).
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// `THIRD_PARTY_LICENSES.md` : la mention de PDFium puis un tableau par source.
pub fn render_markdown(entries: &[Entry]) -> String {
    let mut md = String::from(
        "# Licences des composants tiers\n\n\
         Fichier généré par `cargo xtask licenses` : ne pas le modifier à la main.\n\n\
         MegaUltraPDF est distribué sous licence GPL-3.0-or-later. Il embarque **PDFium** \
         (Google, licence BSD-3-Clause), compilé par le projet pdfium-binaries (MIT) ; \
         le texte de ces licences et de celles des bibliothèques incluses dans PDFium est \
         livré avec l'application, à côté de la bibliothèque PDFium.\n\n\
         Il incorpore aussi la police **DejaVu Sans** 2.37 (licence Bitstream Vera, \
         modifications DejaVu dans le domaine public) pour écrire le grec, le cyrillique et \
         le latin étendu dans les PDF ; sa licence complète figure plus bas.\n\n\
         Les tableaux ci-dessous couvrent toutes les plateformes pour les crates, et les \
         outils de construction : ils sont un sur-ensemble de ce qui est réellement embarqué. \
         Les textes complets des licences suivent les tableaux.\n",
    );
    for (source, title) in [(Source::Crate, "Crates Rust"), (Source::Npm, "Paquets npm")] {
        let rows: Vec<&Entry> = entries.iter().filter(|e| e.source == source).collect();
        md.push_str(&format!(
            "\n## {title} ({})\n\n| Nom | Version | Licence |\n|---|---|---|\n",
            rows.len()
        ));
        for e in rows {
            let license = e.license.as_deref().unwrap_or("voir fichier");
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                cell(&e.name),
                cell(&e.version),
                cell(license)
            ));
        }
    }
    md.push_str("\n## Police DejaVu Sans 2.37\n\n```text\n");
    md.push_str(DEJAVU_LICENSE.trim_end());
    md.push_str("\n```\n");
    md
}

/// Licence de la police DejaVu Sans incorporée dans `mega-core` (Lot 10).
const DEJAVU_LICENSE: &str = include_str!("../../crates/mega-core/assets/fonts/LICENSE-DejaVu.txt");

/// Fichiers de licence d'un paquet : ceux de son dossier (non récursif) dont le nom commence
/// par `LICENSE`, `LICENCE`, `COPYING`, `NOTICE` ou `UNLICENSE`, quelle que soit la casse,
/// triés par nom.
pub fn license_files(dir: &Path) -> Vec<PathBuf> {
    const PREFIXES: [&str; 5] = ["LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE"];
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        // `DirEntry::file_type` ne suit pas les liens symboliques : un « LICENSE » qui
        // pointerait vers un fichier privé de la machine n'est jamais lu (ni publié).
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_uppercase())
                .unwrap_or_default();
            PREFIXES.iter().any(|prefix| name.starts_with(prefix))
        })
        .collect();
    files.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    files
}

/// Taille maximale lue dans un fichier de licence : un paquet hostile ne doit pas pouvoir
/// gonfler le générateur ni le fichier livré.
pub const MAX_LICENSE_BYTES: usize = 256 * 1024;

/// Texte d'un fichier de licence, lu au plus sur [`MAX_LICENSE_BYTES`] octets.
pub fn read_license_text(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let file =
        std::fs::File::open(path).with_context(|| format!("lecture de {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_LICENSE_BYTES as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("lecture de {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Regroupe les textes identiques (fins de ligne normalisées) : chaque texte n'est imprimé
/// qu'une fois, suivi des paquets qui l'utilisent (triés). Groupes triés par premier paquet.
pub fn group_texts(items: Vec<(String, String)>) -> Vec<(String, Vec<String>)> {
    let mut by_text: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (label, text) in items {
        let normalized = text.replace("\r\n", "\n").trim().to_string();
        by_text.entry(normalized).or_default().push(label);
    }
    let mut groups: Vec<(String, Vec<String>)> = by_text
        .into_iter()
        .map(|(text, mut labels)| {
            labels.sort();
            labels.dedup();
            (text, labels)
        })
        .collect();
    groups.sort_by(|a, b| a.1.first().cmp(&b.1.first()));
    groups
}

/// Nom de paquet sûr pour un titre Markdown : sur une ligne, sans accent grave.
fn plain(label: &str) -> String {
    label
        .replace(['\r', '\n'], " ")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Longueur de la plus longue suite d'accents graves dans `text`.
fn longest_backtick_run(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Section « Textes des licences » : chaque texte une fois, puis les paquets qui ne livrent
/// aucun fichier de licence (leur licence déclarée figure dans les tableaux).
pub fn render_texts(groups: &[(String, Vec<String>)], without_file: &[String]) -> String {
    let mut md = String::from("\n## Textes des licences\n");
    for (text, labels) in groups {
        let names: Vec<String> = labels.iter().map(|l| plain(l)).collect();
        // Clôture plus longue que toute suite d'accents graves du texte : un texte venu d'un
        // paquet ne peut pas fermer le bloc et injecter du Markdown.
        let fence = "`".repeat(longest_backtick_run(text).max(2) + 1);
        md.push_str(&format!(
            "\n### {}\n\n{fence}text\n{text}\n{fence}\n",
            names.join(", ")
        ));
    }
    let without_file: Vec<String> = without_file.iter().map(|l| plain(l)).collect();
    if !without_file.is_empty() {
        md.push_str(&format!(
            "\n### Paquets sans fichier de licence ({})\n\n\
             Ces paquets ne livrent pas de texte, ou n'ont pas été téléchargés sur la machine \
             qui a produit ce fichier (crates propres à une autre plateforme) ; leur licence \
             déclarée figure dans les tableaux ci-dessus : {}.\n",
            without_file.len(),
            without_file.join(", ")
        ));
    }
    md
}

/// Lance un programme et retourne sa sortie standard (erreur si le code de sortie n'est pas 0).
fn run(cmd: &mut Command, what: &str) -> anyhow::Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("impossible de lancer {what}"))?;
    if !out.status.success() {
        bail!(
            "{what} a échoué : {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).with_context(|| format!("sortie de {what} non UTF-8"))
}

/// `pnpm` est un script `.cmd` sous Windows : il faut passer par `cmd /C`.
fn pnpm() -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "pnpm"]);
        c
    } else {
        Command::new("pnpm")
    }
}

/// Arguments de `pnpm` pour l'inventaire npm : les seules dépendances **livrées** (`--prod`),
/// pas les outils de développement (Vite, eslint…), qui ne sont pas distribués (C31).
fn npm_license_args() -> [&'static str; 6] {
    ["--dir", "app", "licenses", "list", "--json", "--prod"]
}

/// Génère les deux fichiers d'inventaire depuis la racine du workspace.
pub fn generate(root: &Path) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let metadata = run(
        Command::new(cargo)
            .args(["metadata", "--format-version", "1", "--locked"])
            .current_dir(root),
        "cargo metadata",
    )?;
    let npm = run(
        pnpm().args(npm_license_args()).current_dir(root),
        "pnpm licenses list",
    )?;
    let mut entries = crate_entries_from_metadata(&metadata)?;
    entries.extend(npm_entries(&npm)?);
    let entries = sorted_unique(entries);

    // Textes des licences, lus dans le dossier de chaque paquet (registre Cargo, node_modules).
    let mut texts = Vec::new();
    let mut without_file = Vec::new();
    for e in &entries {
        let label = format!("{} {}", e.name, e.version);
        let files = e.dir.as_deref().map(license_files).unwrap_or_default();
        if files.is_empty() {
            without_file.push(label);
            continue;
        }
        for file in files {
            texts.push((label.clone(), read_license_text(&file)?));
        }
    }
    let groups = group_texts(texts);

    let generated = root.join("app/src/lib/generated");
    std::fs::create_dir_all(&generated)?;
    let json = serde_json::to_string_pretty(&entries)? + "\n";
    std::fs::write(generated.join("licenses.json"), json)?;
    std::fs::write(
        root.join("THIRD_PARTY_LICENSES.md"),
        render_markdown(&entries) + &render_texts(&groups, &without_file),
    )?;
    let crates = entries.iter().filter(|e| e.source == Source::Crate).count();
    println!(
        "Inventaire écrit : {crates} crates, {} paquets npm, {} textes de licence distincts, \
         {} paquets sans fichier de licence.",
        entries.len() - crates,
        groups.len(),
        without_file.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const METADATA: &str = r#"{
      "workspace_members": ["path+file:///repo/crates/mega-core#0.1.0"],
      "packages": [
        {"id": "path+file:///repo/crates/mega-core#0.1.0", "name": "mega-core",
         "version": "0.1.0", "license": "GPL-3.0-or-later", "license_file": null},
        {"id": "registry+x#serde@1.0.200", "name": "serde", "version": "1.0.200",
         "license": "MIT OR Apache-2.0", "license_file": null},
        {"id": "registry+x#ring@0.17.8", "name": "ring", "version": "0.17.8",
         "license": null, "license_file": "LICENSE"}
      ]
    }"#;

    #[test]
    fn crate_entries_skip_workspace_members() {
        let entries = crate_entries_from_metadata(METADATA).unwrap();
        assert!(entries.iter().all(|e| e.name != "mega-core"));
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.source == Source::Crate));
    }

    #[test]
    fn crate_without_license_field_is_listed_as_see_file() {
        let entries = crate_entries_from_metadata(METADATA).unwrap();
        let ring = entries.iter().find(|e| e.name == "ring").unwrap();
        assert_eq!(ring.license, None);
        assert!(render_markdown(&entries).contains("| ring | 0.17.8 | voir fichier |"));
    }

    #[test]
    fn npm_entries_expand_every_version() {
        let json = r#"{
          "MIT": [
            {"name": "svelte", "versions": ["5.1.0"], "license": "MIT", "paths": []},
            {"name": "debug", "versions": ["2.6.9", "4.3.4"], "license": "MIT", "paths": []}
          ],
          "Apache-2.0 OR MIT": [
            {"name": "@tauri-apps/api", "versions": ["2.12.4"], "license": "Apache-2.0 OR MIT"}
          ]
        }"#;
        let entries = npm_entries(json).unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().all(|e| e.source == Source::Npm));
        let debug: Vec<&str> = entries
            .iter()
            .filter(|e| e.name == "debug")
            .map(|e| e.version.as_str())
            .collect();
        assert_eq!(debug, ["2.6.9", "4.3.4"]);
    }

    fn entry(name: &str, version: &str, source: Source) -> Entry {
        Entry {
            name: name.into(),
            version: version.into(),
            license: Some("MIT".into()),
            source,
            dir: None,
        }
    }

    #[test]
    fn inventory_is_sorted_and_deduplicated() {
        let sorted = sorted_unique(vec![
            entry("zeta", "1.0.0", Source::Crate),
            entry("Alpha", "2.0.0", Source::Crate),
            entry("zeta", "1.0.0", Source::Crate),
            entry("alpha", "1.0.0", Source::Npm),
        ]);
        let names: Vec<(&str, &str)> = sorted
            .iter()
            .map(|e| (e.name.as_str(), e.version.as_str()))
            .collect();
        assert_eq!(
            names,
            [("alpha", "1.0.0"), ("Alpha", "2.0.0"), ("zeta", "1.0.0")]
        );
    }

    #[test]
    fn markdown_has_one_section_per_source_and_the_pdfium_notice() {
        let md = render_markdown(&[
            entry("serde", "1.0.200", Source::Crate),
            entry("svelte", "5.1.0", Source::Npm),
        ]);
        assert!(md.contains("PDFium"));
        assert!(md.contains("## Crates Rust (1)"));
        assert!(md.contains("## Paquets npm (1)"));
        assert!(md.find("| serde |").unwrap() < md.find("## Paquets npm").unwrap());
    }

    /// Lot 10 : la police DejaVu Sans incorporée est citée, avec le texte de sa licence.
    #[test]
    fn markdown_mentions_the_embedded_dejavu_font() {
        let md = render_markdown(&[]);
        assert!(md.contains("DejaVu Sans"), "mention");
        assert!(md.contains("Bitstream Vera"), "texte de la licence");
    }

    #[test]
    fn license_files_are_found_by_name() {
        let dir = std::env::temp_dir().join(format!("mega-lic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        for f in [
            "LICENSE-MIT",
            "COPYING",
            "license.txt",
            "README.md",
            "Cargo.toml",
        ] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let names: Vec<String> = license_files(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["COPYING", "LICENSE-MIT", "license.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn identical_license_texts_are_printed_once() {
        let groups = group_texts(vec![
            ("b 1.0".into(), "MIT text\r\n".into()),
            ("a 2.0".into(), "MIT text\n".into()),
            ("c 1.0".into(), "Apache text".into()),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].1, ["a 2.0", "b 1.0"]);
        let md = render_texts(&groups, &["d 0.1".to_string()]);
        assert_eq!(md.matches("MIT text").count(), 1);
        assert!(md.contains("Apache text"));
        // Un paquet sans fichier de licence est nommé, pas oublié.
        assert!(md.contains("d 0.1"));
    }

    #[test]
    fn license_files_skip_symlinks() {
        let dir = std::env::temp_dir().join(format!("mega-lic-ln-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let secret = dir.join("secret.txt");
        std::fs::write(&secret, "private").unwrap();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&secret, dir.join("LICENSE"));
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&secret, dir.join("LICENSE"));
        // Sous Windows, créer un lien demande un privilège : sans lien, rien à vérifier.
        if made.is_ok() {
            assert!(license_files(&dir).is_empty());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn license_text_reading_is_capped() {
        let dir = std::env::temp_dir().join(format!("mega-lic-big-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("LICENSE");
        std::fs::write(&file, "a".repeat(MAX_LICENSE_BYTES + 10)).unwrap();
        assert_eq!(read_license_text(&file).unwrap().len(), MAX_LICENSE_BYTES);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn license_text_cannot_break_out_of_its_fence() {
        let groups = vec![(
            "fence ``` inside\n## injected".to_string(),
            vec!["evil 1.0\n# heading `x`".to_string()],
        )];
        let md = render_texts(&groups, &["bad\nname 2.0".to_string()]);
        assert!(md.contains("````text\nfence ``` inside\n## injected\n````"));
        assert!(md.contains("### evil 1.0 # heading x"));
        assert!(md.contains("bad name 2.0"));
    }

    #[test]
    fn pipes_in_license_expressions_are_escaped() {
        let mut e = entry("x", "1.0.0", Source::Npm);
        e.license = Some("MIT | Apache-2.0".into());
        assert!(render_markdown(&[e]).contains(r"| MIT \| Apache-2.0 |"));
    }
}

#[cfg(test)]
mod prod_tests {
    use super::*;

    /// C31 : l'inventaire ne liste que les dépendances livrées (pas les outils de dev).
    #[test]
    fn npm_licenses_list_production_dependencies_only() {
        assert!(npm_license_args().contains(&"--prod"));
        assert!(npm_license_args().contains(&"--json"));
    }
}
