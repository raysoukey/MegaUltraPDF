// SPDX-License-Identifier: GPL-3.0-or-later
//! Lecteur PDF par défaut (Lot 11, ADR-0015) : savoir si MegaUltraPDF ouvre les PDF du
//! système, et aider l'utilisateur à le choisir.
//!
//! Ce n'est pas de la logique PDF mais une intégration au système d'exploitation : elle vit
//! dans l'application, pas dans `mega-core`. Chaque système a ses règles :
//! - **Windows 10/11** interdit à une application de se désigner elle-même (le choix
//!   `UserChoice` est protégé par une empreinte) : on lit le choix actuel et on ouvre les
//!   **Paramètres** sur la fiche de l'application ;
//! - **Linux** : `xdg-mime` lit et change le choix directement ;
//! - **macOS** : le choix se fait dans le Finder (Lire les informations, Ouvrir avec, Tout
//!   modifier) ; l'interface en donne la marche à suivre.
//!
//! Les programmes (`reg`, `xdg-mime`) sont lancés **sans shell**, arguments en liste.
//!
//! \[Apprentissage\] **`cfg!` contre `#[cfg]`** : `cfg!(windows)` est une expression booléenne
//! évaluée à la compilation, mais les deux branches doivent compiler partout ; `#[cfg(windows)]`
//! retire carrément le code des autres systèmes. On garde les fonctions d'analyse (pures)
//! compilées partout pour les tester sur toutes les machines de la CI.

use std::path::Path;

use serde::Serialize;

/// Identifiant de type de fichier (ProgId) que l'installateur Windows associe à `.pdf` : le
/// nom de l'association de `tauri.conf.json`.
pub const PROG_ID: &str = "MegaUltraPDF.Document";
/// Nom sous lequel l'installateur Windows enregistre l'application (`RegisteredApplications`,
/// greffon `windows/hooks.nsh`) : la page des Paramètres s'ouvre sur sa fiche.
pub const REGISTERED_APP: &str = "MegaUltraPDF";
/// Fichier `.desktop` installé par le paquet Linux (`<productName>.desktop`).
pub const DESKTOP_FILE: &str = "MegaUltraPDF.desktop";

/// Clé du registre où Windows range le choix de l'utilisateur pour `.pdf`.
const USER_CHOICE: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf\UserChoice";

/// Qui ouvre les PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAppKind {
    /// MegaUltraPDF.
    Default,
    /// Un autre programme.
    Other,
    /// On ne sait pas le lire sur ce système (macOS) ou aucun choix n'est enregistré.
    Unknown,
}

/// État présenté dans les préférences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultAppStatus {
    /// Qui ouvre les PDF.
    pub kind: DefaultAppKind,
    /// Identifiant lu (ProgId Windows, fichier `.desktop` Linux), pour l'information.
    pub handler: Option<String>,
    /// Version portable (`.zip` Windows, AppImage Linux) : rien d'installé, aucune
    /// association possible.
    pub portable: bool,
}

/// Ce que [`make_default`] a fait, pour le message de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAppAction {
    /// Les Paramètres de Windows sont ouverts : l'utilisateur y choisit MegaUltraPDF.
    OpenedSettings,
    /// MegaUltraPDF est désormais le lecteur par défaut (Linux).
    Set,
    /// Marche à suivre à afficher (macOS).
    Instructions,
    /// Impossible ici (version portable, outil système absent).
    Unsupported,
}

/// Le ProgId d'une sortie de `reg query … /v ProgId` (ligne `ProgId    REG_SZ    valeur`).
pub fn parse_reg_progid(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Le fichier `.desktop` d'une sortie de `xdg-mime query default application/pdf`.
pub fn parse_xdg_default(output: &str) -> Option<String> {
    let value = output.lines().next()?.trim();
    value.ends_with(".desktop").then(|| value.to_string())
}

/// Qui ouvre les PDF, d'après l'identifiant lu : notre ProgId (ou notre exécutable choisi par
/// « Ouvrir avec… », `Applications\mega-app.exe`) ou notre fichier `.desktop`.
pub fn classify(handler: Option<&str>) -> DefaultAppKind {
    let Some(handler) = handler else {
        return DefaultAppKind::Unknown;
    };
    let lower = handler.to_ascii_lowercase();
    if lower == PROG_ID.to_ascii_lowercase()
        || lower.contains("mega-app.exe")
        || handler == DESKTOP_FILE
    {
        DefaultAppKind::Default
    } else {
        DefaultAppKind::Other
    }
}

/// Vrai si l'exécutable du dossier `exe_dir` est une version **portable** Windows : sans
/// désinstalleur à côté, rien n'a été installé ni associé.
pub fn is_portable_windows(exe_dir: &Path) -> bool {
    !exe_dir.join("uninstall.exe").is_file()
}

/// Lance `program args…` sans shell et rend sa sortie standard s'il réussit.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(windows)]
    {
        // Pas de fenêtre de console qui clignote (comme pour Tesseract).
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Vrai pour une version portable sur ce système.
fn portable() -> bool {
    if cfg!(windows) {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(is_portable_windows))
            .unwrap_or(true)
    } else if cfg!(target_os = "linux") {
        // Une AppImage se lance sans être installée ; elle expose son chemin ici.
        std::env::var_os("APPIMAGE").is_some()
    } else {
        false
    }
}

/// Le choix actuel du système pour les PDF.
pub fn status() -> DefaultAppStatus {
    let handler = if cfg!(windows) {
        run("reg", &["query", USER_CHOICE, "/v", "ProgId"])
            .as_deref()
            .and_then(parse_reg_progid)
    } else if cfg!(target_os = "linux") {
        run("xdg-mime", &["query", "default", "application/pdf"])
            .as_deref()
            .and_then(parse_xdg_default)
    } else {
        None
    };
    DefaultAppStatus {
        kind: classify(handler.as_deref()),
        handler,
        portable: portable(),
    }
}

/// Aide à faire de MegaUltraPDF le lecteur par défaut, selon ce que le système permet.
/// `open_url` ouvre une adresse (les Paramètres de Windows) et dit si c'est fait.
pub fn make_default(open_url: impl Fn(&str) -> bool) -> DefaultAppAction {
    if cfg!(target_os = "macos") {
        return DefaultAppAction::Instructions;
    }
    if portable() {
        return DefaultAppAction::Unsupported;
    }
    if cfg!(windows) {
        let page = format!("ms-settings:defaultapps?registeredAppUser={REGISTERED_APP}");
        if open_url(&page) || open_url("ms-settings:defaultapps") {
            DefaultAppAction::OpenedSettings
        } else {
            DefaultAppAction::Unsupported
        }
    } else if run("xdg-mime", &["default", DESKTOP_FILE, "application/pdf"]).is_some() {
        DefaultAppAction::Set
    } else {
        DefaultAppAction::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_query_output_gives_the_prog_id() {
        let output = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.pdf\\UserChoice\r\n    ProgId    REG_SZ    MSEdgePDF\r\n\r\n";
        assert_eq!(parse_reg_progid(output).as_deref(), Some("MSEdgePDF"));
        assert_eq!(parse_reg_progid("Erreur : clé introuvable"), None);
        assert_eq!(parse_reg_progid("    ProgId    REG_SZ    "), None);
    }

    #[test]
    fn xdg_mime_output_gives_the_desktop_file() {
        assert_eq!(
            parse_xdg_default("org.gnome.Evince.desktop\n").as_deref(),
            Some("org.gnome.Evince.desktop")
        );
        assert_eq!(parse_xdg_default(""), None);
        assert_eq!(parse_xdg_default("pas un fichier"), None);
    }

    #[test]
    fn classify_recognises_our_handlers() {
        assert_eq!(
            classify(Some("MegaUltraPDF.Document")),
            DefaultAppKind::Default
        );
        assert_eq!(
            classify(Some("megaultrapdf.document")),
            DefaultAppKind::Default
        );
        assert_eq!(
            classify(Some(r"Applications\mega-app.exe")),
            DefaultAppKind::Default
        );
        assert_eq!(
            classify(Some("MegaUltraPDF.desktop")),
            DefaultAppKind::Default
        );
        assert_eq!(classify(Some("MSEdgePDF")), DefaultAppKind::Other);
        assert_eq!(classify(None), DefaultAppKind::Unknown);
    }

    #[test]
    fn portable_windows_has_no_uninstaller() {
        let dir = std::env::temp_dir().join(format!("mega-portable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(is_portable_windows(&dir));
        std::fs::write(dir.join("uninstall.exe"), b"MZ").unwrap();
        assert!(!is_portable_windows(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_serialises_for_the_interface() {
        let status = DefaultAppStatus {
            kind: DefaultAppKind::Other,
            handler: Some("MSEdgePDF".to_string()),
            portable: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"other","handler":"MSEdgePDF","portable":false}"#
        );
        assert_eq!(
            serde_json::to_string(&DefaultAppAction::OpenedSettings).unwrap(),
            r#""opened_settings""#
        );
    }
}
