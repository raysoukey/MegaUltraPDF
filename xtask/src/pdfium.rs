// SPDX-License-Identifier: GPL-3.0-or-later
//! Téléchargement de la bibliothèque PDFium (version épinglée, somme de contrôle vérifiée).
//!
//! PDFium est le seul composant non-Rust du projet. On ne le commite pas (binaire de
//! plusieurs Mo) : on télécharge une version précise, on vérifie son empreinte, et on
//! la range dans `vendor/pdfium/<triple>/` où `mega-core` sait la trouver.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Contenu de `xtask/pdfium.lock.toml`, inclus dans le binaire à la compilation.
///
/// `include_str!` lit le fichier *pendant la compilation* : le binaire `xtask`
/// porte donc toujours la version épinglée du dépôt, même lancé d'ailleurs.
const LOCK_TOML: &str = include_str!("../pdfium.lock.toml");

const RELEASE_BASE_URL: &str = "https://github.com/bblanchon/pdfium-binaries/releases/download";

/// Une archive PDFium publiée pour une ou plusieurs cibles de compilation.
#[derive(Debug, Deserialize)]
pub struct Asset {
    /// Triples Rust servis par cette archive (ex. `x86_64-pc-windows-msvc`).
    pub targets: Vec<String>,
    /// Nom du fichier dans la release GitHub.
    pub file_name: String,
    /// Empreinte SHA-256 attendue de l'archive, en hexadécimal minuscule.
    pub sha256: String,
    /// Chemin de la bibliothèque dynamique à l'intérieur de l'archive.
    pub library_path_in_archive: String,
}

/// Le fichier de verrouillage complet : un tag de release et ses archives.
#[derive(Debug, Deserialize)]
struct Lock {
    tag: String,
    #[serde(rename = "asset")]
    assets: Vec<Asset>,
}

/// Désérialise le verrou une seule fois, à la première demande.
///
/// \[Apprentissage\] `OnceLock` : une valeur initialisée paresseusement, une seule fois,
/// sûre entre threads. On obtient une référence `'static` sans variable mutable globale.
/// Le même outil sert dans `mega-core` pour l'instance PDFium.
fn lock() -> &'static Lock {
    static LOCK: OnceLock<Lock> = OnceLock::new();
    LOCK.get_or_init(|| toml::from_str(LOCK_TOML).expect("pdfium.lock.toml est invalide"))
}

/// Tag de la release épinglée (ex. `chromium/7881`).
pub fn pinned_tag() -> &'static str {
    &lock().tag
}

/// Trouve l'archive qui sert le triple de compilation donné, s'il est pris en charge.
pub fn asset_for_target(target: &str) -> Option<&'static Asset> {
    lock()
        .assets
        .iter()
        .find(|a| a.targets.iter().any(|t| t == target))
}

/// Vérifie que `bytes` a bien l'empreinte SHA-256 `expected_hex`.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> anyhow::Result<()> {
    let actual = Sha256::digest(bytes);
    let actual_hex = format!("{actual:x}");
    if actual_hex != expected_hex.to_ascii_lowercase() {
        bail!("somme SHA-256 inattendue : attendu {expected_hex}, obtenu {actual_hex}");
    }
    Ok(())
}

/// Fichier marqueur de version, **par cible** : `vendor/pdfium/<target>/VERSION`.
///
/// Un marqueur unique pour toutes les cibles faisait croire qu'une cible était à jour
/// dès qu'une autre venait d'être téléchargée avec le nouveau tag.
pub fn version_marker_path(vendor_dir: &Path, target: &str) -> PathBuf {
    vendor_dir.join(target).join("VERSION")
}

/// Nom du fichier de bibliothèque (dernier segment du chemin dans l'archive).
pub fn library_file_name(asset: &Asset) -> &str {
    asset
        .library_path_in_archive
        .rsplit('/')
        .next()
        .unwrap_or(&asset.library_path_in_archive)
}

/// Télécharge (si nécessaire) la bibliothèque PDFium pour `target` dans
/// `<workspace_root>/vendor/pdfium/<target>/` et retourne son chemin.
pub fn fetch(workspace_root: &Path, target: &str) -> anyhow::Result<PathBuf> {
    let asset = asset_for_target(target)
        .with_context(|| format!("aucune archive PDFium connue pour la cible {target}"))?;
    let tag = pinned_tag();
    let vendor_dir = workspace_root.join("vendor").join("pdfium");
    let dest_dir = vendor_dir.join(target);
    let dest = dest_dir.join(library_file_name(asset));
    let version_file = version_marker_path(&vendor_dir, target);

    let notice = dest_dir.join(NOTICE_FILE);
    let already_present = dest.exists()
        && notice.exists()
        && std::fs::read_to_string(&version_file)
            .map(|v| v.trim() == tag)
            .unwrap_or(false);
    // Ancien emplacement (partagé entre cibles, avant la révision du Lot 0) : on l'ignore,
    // ce qui force un retéléchargement unique, puis on le supprime.
    let legacy_marker = vendor_dir.join("VERSION");
    if legacy_marker.exists() {
        let _ = std::fs::remove_file(&legacy_marker);
    }
    if already_present {
        println!("PDFium {tag} déjà présent : {}", dest.display());
        return Ok(dest);
    }

    let url = format!(
        "{RELEASE_BASE_URL}/{}/{}",
        tag.replace('/', "%2F"),
        asset.file_name
    );
    println!("Téléchargement de {url} …");
    let mut body = Vec::new();
    ureq::get(&url)
        .call()
        .with_context(|| format!("échec du téléchargement de {url}"))?
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .context("lecture de la réponse HTTP")?;
    println!("Reçu {} octets, vérification de l'empreinte …", body.len());
    verify_sha256(&body, &asset.sha256)?;

    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("création de {}", dest_dir.display()))?;
    let notices = extract_library_and_notices(&body, &asset.library_path_in_archive, &dest)?;
    std::fs::write(&notice, combine_notices(&notices))
        .with_context(|| format!("écriture de {}", notice.display()))?;
    std::fs::write(&version_file, format!("{tag}\n"))?;
    println!("PDFium {tag} installé : {}", dest.display());
    Ok(dest)
}

/// Nom du fichier de licences livré à côté de la bibliothèque (et embarqué par le bundler).
pub const NOTICE_FILE: &str = "LICENSE";

/// Une entrée de l'archive fait-elle partie des licences à livrer ? (`LICENSE` à la racine,
/// licence de l'empaqueteur pdfium-binaries, et chaque fichier de `licenses/` : PDFium et les
/// bibliothèques qu'il inclut.)
pub fn is_notice_entry(path: &str) -> bool {
    path == "LICENSE" || (path.starts_with("licenses/") && path.len() > "licenses/".len())
}

/// Assemble les licences en un seul texte : `LICENSE` d'abord, puis `licenses/pdfium.txt`,
/// puis les autres par ordre alphabétique, chacune précédée de son chemin dans l'archive.
pub fn combine_notices(notices: &[(String, String)]) -> String {
    let rank = |p: &str| match p {
        "LICENSE" => 0,
        "licenses/pdfium.txt" => 1,
        _ => 2,
    };
    let mut sorted: Vec<&(String, String)> = notices.iter().collect();
    sorted.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then_with(|| a.0.cmp(&b.0)));
    let mut out = String::from(
        "Licences de PDFium et des bibliothèques qu'il inclut (archive pdfium-binaries).
",
    );
    for (path, text) in sorted {
        out.push_str(&format!(
            "
===== {path} =====

{}
",
            text.trim_end()
        ));
    }
    out
}

/// Texte d'un fichier de licence : UTF-8 s'il est valide, sinon Latin-1 (cas réel :
/// `licenses/freetype.txt` de l'archive), où chaque octet est un caractère.
pub fn decode_notice(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Extrait la bibliothèque `wanted` vers `dest` et retourne le texte des licences
/// (`is_notice_entry`) trouvées dans la même archive `.tgz` en mémoire.
fn extract_library_and_notices(
    tgz: &[u8],
    wanted: &str,
    dest: &Path,
) -> anyhow::Result<Vec<(String, String)>> {
    let gz = flate2::read::GzDecoder::new(tgz);
    let mut archive = tar::Archive::new(gz);
    let mut found = false;
    let mut notices = Vec::new();
    for entry in archive.entries().context("lecture de l'archive tar")? {
        let mut entry = entry?;
        let path = entry
            .path()?
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string();
        if path == wanted {
            let mut out = std::fs::File::create(dest)
                .with_context(|| format!("création de {}", dest.display()))?;
            std::io::copy(&mut entry, &mut out)?;
            found = true;
        } else if is_notice_entry(&path) {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .with_context(|| format!("lecture de {path} dans l'archive"))?;
            notices.push((path, decode_notice(&bytes)));
        }
    }
    if !found {
        bail!("fichier {wanted} introuvable dans l'archive");
    }
    if notices.is_empty() {
        bail!("aucun fichier de licence dans l'archive PDFium");
    }
    Ok(notices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_target_maps_to_win_x64_asset() {
        let a = asset_for_target("x86_64-pc-windows-msvc").unwrap();
        assert_eq!(a.file_name, "pdfium-win-x64.tgz");
        assert_eq!(a.library_path_in_archive, "bin/pdfium.dll");
    }

    #[test]
    fn unknown_target_is_none() {
        assert!(asset_for_target("wasm32-unknown-unknown").is_none());
    }

    #[test]
    fn sha256_mismatch_is_error() {
        assert!(verify_sha256(b"hello", "00").is_err());
        assert!(
            verify_sha256(
                b"hello",
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn combined_notice_starts_with_the_archive_license_then_components() {
        let notice = combine_notices(&[
            ("licenses/zlib.txt".into(), "zlib text".into()),
            ("LICENSE".into(), "packager text".into()),
            ("licenses/pdfium.txt".into(), "pdfium text".into()),
        ]);
        let packager = notice.find("packager text").unwrap();
        let pdfium = notice.find("pdfium text").unwrap();
        let zlib = notice.find("zlib text").unwrap();
        assert!(packager < pdfium && pdfium < zlib);
        assert!(notice.contains("licenses/pdfium.txt"));
    }

    #[test]
    fn notice_text_falls_back_to_latin1() {
        assert_eq!(decode_notice("café".as_bytes()), "café");
        // « Ré » en Latin-1 : 0x52 0xE9 (pas de l'UTF-8 valide).
        assert_eq!(decode_notice(&[0x52, 0xE9]), "Ré");
    }

    #[test]
    fn notice_entries_are_the_root_license_and_the_licenses_folder() {
        assert!(is_notice_entry("LICENSE"));
        assert!(is_notice_entry("licenses/icu.txt"));
        assert!(!is_notice_entry("licenses/"));
        assert!(!is_notice_entry("bin/pdfium.dll"));
        assert!(!is_notice_entry("include/fpdfview.h"));
    }

    #[test]
    fn version_marker_is_per_target() {
        let vendor = Path::new("vendor/pdfium");
        let a = version_marker_path(vendor, "x86_64-pc-windows-msvc");
        let b = version_marker_path(vendor, "aarch64-apple-darwin");
        assert_ne!(a, b);
        assert!(a.starts_with(vendor.join("x86_64-pc-windows-msvc")));
        assert!(a.ends_with("VERSION"));
    }
}
