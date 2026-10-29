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
//! Le registre Windows se lit par l'API système (crate `winreg`, texte UTF-16) ; `xdg-mime`
//! est lancé **sans shell**, arguments en liste.
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

/// Clé du registre où Windows range le choix de l'utilisateur pour `.pdf` ; Windows 11
/// tient aussi `UserChoiceLatest`, plus récent, lu en premier (relecture du Lot 11).
const USER_CHOICE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf\UserChoice";
/// Commande d'ouverture du ProgId, écrite par les installateurs NSIS et MSI (`%1` à la fin).
const OPEN_COMMAND: &str = r"Software\Classes\MegaUltraPDF.Document\shell\open\command";
/// Durée maximale d'un programme système interrogé (`xdg-mime`) : au-delà, on renonce
/// plutôt que de figer la fenêtre.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

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

/// Vrai si la commande d'ouverture `command` (`"C:\…\mega-app.exe" "%1"`) lance l'exécutable
/// `exe` (comparaison sans tenir compte de la casse, comme le système de fichiers de Windows).
pub fn command_targets_exe(command: &str, exe: &Path) -> bool {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or_default(),
        None => command.split(' ').next().unwrap_or_default(),
    };
    !program.is_empty() && program.eq_ignore_ascii_case(&exe.to_string_lossy())
}

/// Le ProgId choisi pour `.pdf` d'après `query(clé)` (valeur `ProgId` de la clé) :
/// `UserChoiceLatest` d'abord (Windows 11), puis `UserChoice`.
pub fn windows_handler(query: impl Fn(&str) -> Option<String>) -> Option<String> {
    query(&format!("{USER_CHOICE}Latest")).or_else(|| query(USER_CHOICE))
}

/// Dossiers où un fichier `.desktop` est installé (spécification XDG Base Directory) :
/// `$XDG_DATA_HOME` (sinon `~/.local/share`), puis chaque dossier de `$XDG_DATA_DIRS` (sinon
/// `/usr/local/share` et `/usr/share`), suivis de `applications`.
pub fn desktop_dirs(
    data_home: Option<&str>,
    home: Option<&str>,
    data_dirs: Option<&str>,
) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    match (data_home.filter(|d| !d.is_empty()), home) {
        (Some(d), _) => dirs.push(Path::new(d).join("applications")),
        (None, Some(h)) => dirs.push(Path::new(h).join(".local/share/applications")),
        (None, None) => {}
    }
    let shared = data_dirs
        .filter(|d| !d.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    dirs.extend(
        shared
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("applications")),
    );
    dirs
}

/// Lance `program args…` sans shell et rend sa sortie standard s'il réussit dans le délai
/// [`TIMEOUT`] (sinon le processus est arrêté et la réponse est `None`).
///
/// \[Apprentissage\] **Attendre avec une limite** : `wait()` attendrait sans fin un
/// programme bloqué ; `try_wait()` demande « fini ? » sans attendre, ce qui permet une boucle
/// avec une échéance.
fn run(program: &str, args: &[&str]) -> Option<String> {
    use std::io::Read;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        // Pas de fenêtre de console qui clignote (comme pour Tesseract).
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let deadline = std::time::Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    status.success().then_some(out)
}

/// Ruche du registre Windows à lire.
#[derive(Debug, Clone, Copy)]
enum Hive {
    /// `HKEY_CURRENT_USER` : installation pour l'utilisateur.
    User,
    /// `HKEY_LOCAL_MACHINE` : installation pour toute la machine.
    Machine,
}

/// Valeur texte `name` (`""` : la valeur par défaut) de la clé `key` du registre, `None` si
/// elle manque ou est vide.
///
/// L'API Windows rend le texte en UTF-16, converti sans perte : « (par défaut) » et un chemin
/// comme `C:\Users\Hélène\…` arrivent intacts, ce que la sortie de `reg.exe` (page de code de
/// la console) ne permettait pas (correctif 0.11.2).
#[cfg(windows)]
fn reg_string(hive: Hive, key: &str, name: &str) -> Option<String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let root = match hive {
        Hive::User => HKEY_CURRENT_USER,
        Hive::Machine => HKEY_LOCAL_MACHINE,
    };
    let value: String = winreg::RegKey::predef(root)
        .open_subkey(key)
        .ok()?
        .get_value(name)
        .ok()?;
    (!value.is_empty()).then_some(value)
}

/// Hors de Windows, pas de registre.
#[cfg(not(windows))]
fn reg_string(_hive: Hive, _key: &str, _name: &str) -> Option<String> {
    None
}

/// Vrai si cet exécutable est **installé** sous Windows : la commande d'ouverture du ProgId
/// (utilisateur, sinon machine) le désigne. Couvre NSIS et MSI (relecture du Lot 11).
fn installed_windows() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    [Hive::User, Hive::Machine].into_iter().any(|hive| {
        reg_string(hive, OPEN_COMMAND, "")
            .is_some_and(|command| command_targets_exe(&command, &exe))
    })
}

/// Vrai pour une version portable sur ce système.
fn portable() -> bool {
    if cfg!(windows) {
        !installed_windows()
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
        windows_handler(|key| reg_string(Hive::User, key, "ProgId"))
    } else if cfg!(target_os = "linux") {
        linux_handler()
    } else {
        None
    };
    DefaultAppStatus {
        kind: classify(handler.as_deref()),
        handler,
        portable: portable(),
    }
}

/// Le fichier `.desktop` qui ouvre les PDF sous Linux.
fn linux_handler() -> Option<String> {
    run("xdg-mime", &["query", "default", "application/pdf"])
        .as_deref()
        .and_then(parse_xdg_default)
}

/// Vrai si notre fichier `.desktop` est installé dans un dossier XDG.
fn desktop_file_installed() -> bool {
    let var = |name: &str| std::env::var(name).ok();
    desktop_dirs(
        var("XDG_DATA_HOME").as_deref(),
        var("HOME").as_deref(),
        var("XDG_DATA_DIRS").as_deref(),
    )
    .iter()
    .any(|dir| dir.join(DESKTOP_FILE).is_file())
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
        // La fiche de l'application n'existe que si l'installateur NSIS l'a enregistrée
        // (`RegisteredApplications`, utilisateur ou machine) ; sinon (MSI), la page générale.
        let registered = [Hive::User, Hive::Machine].into_iter().any(|hive| {
            reg_string(hive, r"Software\RegisteredApplications", REGISTERED_APP).is_some()
        });
        let page = if registered {
            format!("ms-settings:defaultapps?registeredAppUser={REGISTERED_APP}")
        } else {
            "ms-settings:defaultapps".to_string()
        };
        if open_url(&page) {
            DefaultAppAction::OpenedSettings
        } else {
            DefaultAppAction::Unsupported
        }
    } else {
        // Une entrée vers un fichier `.desktop` absent laisserait les PDF sans programme :
        // on vérifie avant, et on relit après.
        if !desktop_file_installed() {
            return DefaultAppAction::Unsupported;
        }
        let _ = run("xdg-mime", &["default", DESKTOP_FILE, "application/pdf"]);
        if linux_handler().as_deref() == Some(DESKTOP_FILE) {
            DefaultAppAction::Set
        } else {
            DefaultAppAction::Unsupported
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Correctif 0.11.2 : une installation n'était pas reconnue sous un Windows français.
    /// `reg.exe` écrit « (par défaut) » et les chemins accentués dans la page de code de la
    /// console, que la lecture en UTF-8 rejetait. Écrit une clé de test, la relit, l'efface.
    #[cfg(windows)]
    #[test]
    fn registry_values_keep_accents() {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let key = format!(r"Software\MegaUltraPDF-test-{}", std::process::id());
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (created, _) = hkcu.create_subkey(&key).unwrap();
        let command = r#""C:\Users\Hélène\MegaUltraPDF\mega-app.exe" "%1""#;
        created.set_value("", &command).unwrap();
        created.set_value("ProgId", &"Éditeur.PDF").unwrap();
        let default = reg_string(Hive::User, &key, "");
        let named = reg_string(Hive::User, &key, "ProgId");
        let missing = reg_string(Hive::User, &key, "Absente");
        let _ = hkcu.delete_subkey_all(&key);
        assert_eq!(default.as_deref(), Some(command));
        assert_eq!(named.as_deref(), Some("Éditeur.PDF"));
        assert_eq!(missing, None);
        assert!(command_targets_exe(
            command,
            Path::new(r"C:\Users\Hélène\MegaUltraPDF\mega-app.exe")
        ));
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

    /// Relecture du Lot 11 (I1) : une installation (NSIS **ou** MSI) se reconnaît à la
    /// commande d'ouverture du ProgId, qui désigne cet exécutable.
    #[test]
    fn open_command_targets_this_executable() {
        let exe = Path::new(r"C:\Program Files\MegaUltraPDF\mega-app.exe");
        assert!(command_targets_exe(
            r#""C:\Program Files\MegaUltraPDF\mega-app.exe" "%1""#,
            exe
        ));
        assert!(command_targets_exe(
            r#""c:\program files\megaultrapdf\MEGA-APP.EXE" "%1""#,
            exe
        ));
        assert!(!command_targets_exe(r#""D:utre\mega-app.exe" "%1""#, exe));
        assert!(!command_targets_exe("", exe));
    }

    /// Relecture du Lot 11 (I4) : Windows 11 range le choix dans `UserChoiceLatest`, plus
    /// récent que `UserChoice` ; on lit d'abord celui-là.
    #[test]
    fn user_choice_latest_wins() {
        let query = |key: &str| {
            if key.ends_with("UserChoiceLatest") {
                Some("MegaUltraPDF.Document".to_string())
            } else {
                Some("MSEdgePDF".to_string())
            }
        };
        assert_eq!(
            windows_handler(query).as_deref(),
            Some("MegaUltraPDF.Document")
        );
        let legacy_only = |key: &str| (!key.ends_with("Latest")).then(|| "MSEdgePDF".to_string());
        assert_eq!(windows_handler(legacy_only).as_deref(), Some("MSEdgePDF"));
    }

    /// Relecture du Lot 11 (I5) : les dossiers où chercher le fichier `.desktop`
    /// (`$XDG_DATA_HOME`, sinon `~/.local/share`, puis `$XDG_DATA_DIRS`, sinon les défauts).
    #[test]
    fn desktop_file_search_dirs_follow_xdg() {
        let dirs = desktop_dirs(None, Some("/home/a"), None);
        assert_eq!(
            dirs,
            vec![
                std::path::PathBuf::from("/home/a/.local/share/applications"),
                std::path::PathBuf::from("/usr/local/share/applications"),
                std::path::PathBuf::from("/usr/share/applications"),
            ]
        );
        let custom = desktop_dirs(Some("/x"), Some("/home/a"), Some("/d1:/d2"));
        assert_eq!(custom[0], std::path::PathBuf::from("/x/applications"));
        assert_eq!(custom[2], std::path::PathBuf::from("/d2/applications"));
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

    /// Un PDF ouvert depuis le gestionnaire de fichiers n'arrive à l'application que si la
    /// ligne `Exec` du fichier `.desktop` porte `%F` (liste des fichiers) ; le modèle par
    /// défaut de Tauri ne le met pas.
    #[test]
    fn desktop_template_passes_the_files() {
        let template = include_str!("../linux/MegaUltraPDF.desktop");
        let exec = template
            .lines()
            .find(|l| l.starts_with("Exec="))
            .unwrap_or_default();
        assert!(exec.ends_with(" %F"), "{exec}");
        assert!(template.contains("MimeType={{mime_type}}"));
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            conf["bundle"]["linux"]["deb"]["desktopTemplate"],
            "linux/MegaUltraPDF.desktop"
        );
    }
}
