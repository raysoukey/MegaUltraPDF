// SPDX-License-Identifier: GPL-3.0-or-later
//! Localisation et initialisation de la bibliothèque PDFium.
//!
//! PDFium est chargée **une seule fois** par processus et partagée par tous les
//! documents. La crate `pdfium-render` (feature `thread_safe`, activée par défaut)
//! sérialise les appels derrière un verrou, car PDFium elle-même n'est pas conçue
//! pour être appelée depuis plusieurs threads à la fois.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::{PdfDocument, Pdfium};

use crate::error::{CoreError, CoreResult};

/// Variable d'environnement permettant d'imposer le dossier contenant PDFium.
pub const LIBRARY_DIR_ENV: &str = "MEGA_PDFIUM_DIR";

/// Dossier de PDFium indiqué par l'application (ressources d'un paquet installé).
static APP_LIBRARY_DIR: OnceLock<PathBuf> = OnceLock::new();

/// L'instance PDFium (ou la liste des dossiers vainement fouillés), créée au premier usage.
static PDFIUM: OnceLock<Result<Pdfium, Vec<PathBuf>>> = OnceLock::new();

/// Indique à `mega-core` où trouver PDFium avant son premier usage : l'application Tauri y
/// passe le dossier de ses ressources, qui n'est pas celui de l'exécutable sur macOS
/// (`Contents/Resources`) ni sous Linux (`/usr/lib/<app>`).
///
/// Retourne `false` si un dossier a déjà été réglé ou si PDFium est déjà chargée : le réglage
/// arriverait trop tard pour servir.
///
/// \[Apprentissage\] Même outil qu'`engine::pdfium` (fiche `oncelock`) : `OnceLock::set` ne
/// réussit qu'une fois. C'est une alternative sûre à `std::env::set_var`, devenue `unsafe` en
/// Rust 2024 parce qu'un autre thread pourrait lire l'environnement au même moment.
pub fn set_library_dir(dir: PathBuf) -> bool {
    if PDFIUM.get().is_some() {
        return false;
    }
    APP_LIBRARY_DIR.set(dir).is_ok()
}

/// Le dossier réglé par [`set_library_dir`], s'il y en a un.
pub fn app_library_dir() -> Option<&'static Path> {
    APP_LIBRARY_DIR.get().map(PathBuf::as_path)
}

/// Ordre de recherche, en fonction pure (testable) : dossier de l'application, variable
/// d'environnement, dossier de l'exécutable, puis dossier `vendor/` du développement.
fn search_paths_with(
    app_dir: Option<&Path>,
    env_dir: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
    dev_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    app_dir
        .map(Path::to_path_buf)
        .into_iter()
        .chain(env_dir)
        .chain(exe_dir)
        .chain(dev_dir)
        .collect()
}

/// Dossiers où chercher la bibliothèque PDFium, dans l'ordre.
///
/// 1. le dossier réglé par l'application ([`set_library_dir`], ressources du paquet) ;
/// 2. le dossier donné par `MEGA_PDFIUM_DIR` ;
/// 3. le dossier de l'exécutable (version portable) ;
/// 4. `vendor/pdfium/<triple>/` à la racine du workspace (cas de `cargo run` et
///    `cargo test`, rempli par `cargo xtask fetch-pdfium`).
pub fn library_search_paths() -> Vec<PathBuf> {
    let env_dir = std::env::var_os(LIBRARY_DIR_ENV).map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from));
    search_paths_with(app_library_dir(), env_dir, exe_dir, dev_library_dir())
}

/// `vendor/pdfium/<triple>/` du workspace, en développement seulement.
///
/// \[Apprentissage\] `#[cfg(debug_assertions)]` retire la fonction **avant** la
/// compilation en profil `release` : le chemin absolu de la machine de compilation
/// (`CARGO_MANIFEST_DIR`) n'existe même pas dans un binaire distribué. `cfg!(…)`, lui,
/// compilait les deux branches et comptait sur l'optimiseur pour retirer la morte (A10).
#[cfg(debug_assertions)]
fn dev_library_dir() -> Option<PathBuf> {
    // `CARGO_MANIFEST_DIR` vaut `<racine>/crates/mega-core` : on remonte de deux niveaux.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    Some(
        workspace_root
            .join("vendor")
            .join("pdfium")
            .join(env!("MEGA_TARGET_TRIPLE")),
    )
}

/// En `release`, aucun dossier de développement.
#[cfg(not(debug_assertions))]
fn dev_library_dir() -> Option<PathBuf> {
    None
}

/// Retourne l'instance PDFium partagée, en la créant au premier appel.
///
/// \[Apprentissage\] `OnceLock<T>` est une case « écrite une seule fois ».
/// `get_or_init(f)` garantit que `f` s'exécute **exactement une fois**, même si
/// plusieurs threads arrivent en même temps : les autres attendent, puis lisent la
/// valeur. C'est indispensable ici : une première version de cette fonction laissait
/// deux threads charger PDFium en parallèle (« le second résultat est jeté, pas
/// grave ») et le processus plantait sur macOS arm64 — PDFium ne tolère pas deux
/// initialisations simultanées. Le test `concurrent_first_use_is_safe` garde ce cas.
///
/// Le résultat d'un échec de chargement est lui aussi mémorisé : si la bibliothèque
/// manque, il faut la récupérer (`cargo xtask fetch-pdfium`) puis relancer le programme.
///
/// Note : `pdfium-render` garde lui-même ses *bindings* (les pointeurs de fonctions de la
/// DLL) dans un `OnceLock` global (voir `Pdfium::new` dans la source de la crate). Notre
/// `OnceLock` garantit donc en plus l'unicité de l'appel d'initialisation de PDFium, qui
/// n'est pas réentrant. Fiche : `docs/apprentissage/concepts/oncelock.md`.
pub(crate) fn pdfium() -> CoreResult<&'static Pdfium> {
    match PDFIUM.get_or_init(load) {
        Ok(p) => Ok(p),
        Err(searched) => Err(CoreError::LibraryNotFound {
            searched: searched.clone(),
        }),
    }
}

/// Jeton prouvant que le thread courant détient l'accès exclusif à PDFium.
///
/// Toute opération complète sur un document (chargement, lecture, rendu, destruction)
/// se fait en tenant ce jeton : voir [`guard`].
pub(crate) type PdfiumGuard = MutexGuard<'static, ()>;

/// Prend l'accès exclusif à PDFium pour la durée d'une opération complète.
///
/// \[Apprentissage\] La feature `thread_safe` de `pdfium-render` verrouille **chaque appel**
/// à PDFium, pas une **séquence** d'appels. Or PDFium garde de l'état global entre deux
/// appels : par exemple le code d'erreur du dernier chargement (`FPDF_GetLastError`),
/// lu dans un second appel — un chargement réussi sur un autre thread pouvait s'intercaler
/// et transformer « fichier invalide » en « erreur inconnue » ; et des utilisations
/// concurrentes plus soutenues allaient jusqu'au plantage (SIGSEGV en CI Linux/macOS).
/// Ce `Mutex` sérialise donc nos opérations entières. Il ne protège aucune donnée Rust,
/// seulement l'ordre des appels natifs : s'il est « empoisonné » par la panique d'un
/// thread, on le reprend tel quel (`into_inner`). Depuis le Lot 1, un seul thread (l'acteur
/// de `service`) appelle ces fonctions : le verrou n'est plus contendu, il est conservé
/// comme ceinture de sécurité (décision 2 de la spec du Lot 1).
///
/// Règle : ne jamais appeler `guard()` en tenant déjà un jeton (le `Mutex` de la
/// bibliothèque standard n'est pas réentrant : ce serait un interblocage). Les
/// fonctions internes qui supposent le jeton déjà pris le reçoivent en paramètre.
pub(crate) fn guard() -> PdfiumGuard {
    static PDFIUM_LOCK: Mutex<()> = Mutex::new(());
    PDFIUM_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Charge un document depuis des octets. Le jeton garantit que le chargement et la
/// lecture éventuelle du code d'erreur ne sont pas entrelacés avec un autre thread.
pub(crate) fn load_bytes(
    _guard: &PdfiumGuard,
    bytes: Vec<u8>,
    password: Option<&str>,
) -> CoreResult<PdfDocument<'static>> {
    Ok(pdfium()?.load_pdf_from_byte_vec(bytes, password)?)
}

/// Charge la bibliothèque ; en cas d'échec, retourne la liste des dossiers essayés.
fn load() -> Result<Pdfium, Vec<PathBuf>> {
    let searched = library_search_paths();
    for dir in &searched {
        let candidate = Pdfium::pdfium_platform_library_name_at_path(dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&candidate) {
            return Ok(Pdfium::new(bindings));
        }
    }
    // Dernier recours : une PDFium installée au niveau du système.
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(_) => Err(searched),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_dir_set_by_the_app_comes_first() {
        let paths = search_paths_with(
            Some(Path::new("ressources")),
            Some(PathBuf::from("env")),
            Some(PathBuf::from("exe")),
            Some(PathBuf::from("vendor")),
        );
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["ressources", "env", "exe", "vendor"]);
        let without_app = search_paths_with(None, None, Some(PathBuf::from("exe")), None);
        assert_eq!(without_app, vec![PathBuf::from("exe")]);
    }

    #[test]
    fn set_library_dir_is_write_once() {
        // Un dossier inoffensif : celui où PDFium est déjà cherché en développement.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let first = set_library_dir(dir.clone());
        // Le premier appel peut échouer si un autre test a déjà chargé PDFium ; le second
        // échoue toujours.
        assert!(!set_library_dir(PathBuf::from("ailleurs")));
        if first {
            assert_eq!(app_library_dir(), Some(dir.as_path()));
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    fn search_paths_end_with_vendor_dir_for_this_target() {
        let last = library_search_paths().pop().unwrap();
        assert!(last.ends_with(env!("MEGA_TARGET_TRIPLE")));
        assert!(last.to_string_lossy().contains("vendor"));
    }

    /// En release (`cargo test --release -p mega-core engine`), aucun chemin de la machine
    /// de compilation ne doit apparaître.
    #[test]
    #[cfg(not(debug_assertions))]
    fn release_search_paths_do_not_embed_build_paths() {
        for p in library_search_paths() {
            let s = p.to_string_lossy();
            assert!(!s.contains("crates") && !s.contains("vendor"), "{s}");
        }
    }
}
