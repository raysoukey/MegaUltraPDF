// SPDX-License-Identifier: GPL-3.0-or-later
//! Journal de diagnostic **local** (audit du Lot 13, R10).
//!
//! Quand une opération échoue chez l'utilisateur (fichier refusé, panique rattrapée par
//! l'acteur, programme externe arrêté), il ne reste aucune trace pour comprendre ce qui s'est
//! passé. Ce module écrit une ligne par erreur dans `diagnostic.log`, dans le dossier de
//! journaux de l'application (`app_log_dir`) : horodatage, catégorie, message. Jamais le
//! contenu d'un document ni son texte : un message d'erreur qui cite une saisie (texte entre
//! guillemets) ou un chemin est masqué par [`sanitize`] avant d'être écrit. Rien n'est envoyé
//! nulle part (pas de télémétrie).
//! L'utilisateur ouvre le dossier depuis Aide ▸ À propos et joint le fichier à un signalement
//! s'il le souhaite.
//!
//! Le fichier ne grossit pas sans fin : au-delà de [`MAX_LOG_BYTES`], il devient
//! `diagnostic.old.log` (l'ancien est remplacé) et un nouveau commence.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::error::{AppError, ErrorKind};

/// Taille maximale du journal courant (1 Mo) ; avec l'ancien, au plus 2 Mo sur le disque.
pub const MAX_LOG_BYTES: u64 = 1_000_000;

/// Longueur maximale d'un message dans le journal, en caractères.
pub const MAX_MESSAGE_CHARS: usize = 300;

/// Nom du journal courant.
const LOG_NAME: &str = "diagnostic.log";

/// Nom du journal précédent, après rotation.
const OLD_LOG_NAME: &str = "diagnostic.old.log";

/// Un journal dans un dossier donné.
pub struct DiagnosticLog {
    dir: PathBuf,
    max_bytes: u64,
    // Sérialise les écritures : l'acteur, les commandes et le crochet de panique peuvent
    // écrire depuis des threads différents.
    lock: Mutex<()>,
}

impl DiagnosticLog {
    /// Un journal dans `dir` (créé à la première écriture), plafonné à `max_bytes`.
    pub fn new(dir: PathBuf, max_bytes: u64) -> Self {
        DiagnosticLog {
            dir,
            max_bytes,
            lock: Mutex::new(()),
        }
    }

    /// Le dossier du journal.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Ajoute une ligne horodatée. Une erreur d'écriture est ignorée : le journal ne doit
    /// jamais faire échouer l'opération qu'il décrit.
    pub fn write(&self, line: &str) {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.append(line);
    }

    /// Comme [`DiagnosticLog::write`], mais abandonne si une écriture est déjà en cours : le
    /// crochet de panique ne doit jamais attendre un verrou (une panique pendant une écriture
    /// le bloquerait pour toujours).
    fn try_write(&self, line: &str) {
        if let Ok(_guard) = self.lock.try_lock() {
            let _ = self.append(line);
        }
    }

    fn append(&self, line: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(LOG_NAME);
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size + line.len() as u64 + 32 > self.max_bytes {
            std::fs::rename(&path, self.dir.join(OLD_LOG_NAME))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        // Une ligne par entrée : les retours à la ligne du message sont aplatis.
        let flat = line.replace(['\r', '\n'], " ");
        writeln!(file, "{} {flat}", utc_timestamp(unix_now()))
    }
}

/// Le journal de l'application, posé une fois au démarrage par [`init`].
static LOG: OnceLock<DiagnosticLog> = OnceLock::new();

/// Installe le journal dans `dir` et un crochet de panique qui y écrit avant le message
/// habituel (sur la console de développement).
pub fn init(dir: PathBuf) {
    if LOG.set(DiagnosticLog::new(dir, MAX_LOG_BYTES)).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(log) = LOG.get() {
            // L'emplacement dans notre code, et le message masqué comme les autres.
            let location = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            log.try_write(&format!("panique ({location}) : {}", sanitize(&payload)));
        }
        previous(info);
    }));
}

/// Le dossier du journal, s'il est installé.
pub fn dir() -> Option<PathBuf> {
    LOG.get().map(|log| log.dir().to_path_buf())
}

/// Vrai si une erreur de cette catégorie mérite une ligne : pas les demandes de mot de passe
/// (le parcours normal d'un fichier protégé), ni « document inconnu » (onglet déjà fermé).
pub fn worth_logging(kind: ErrorKind) -> bool {
    !matches!(
        kind,
        ErrorKind::PasswordRequired | ErrorKind::WrongPassword | ErrorKind::NotFound
    )
}

/// Note l'erreur `error` renvoyée à l'interface, si elle le mérite.
pub fn record(error: &AppError) {
    if let Some(log) = LOG.get()
        && worth_logging(error.kind)
    {
        log.write(&format!("{:?} : {}", error.kind, sanitize(&error.message)));
    }
}

/// Le message `message` sans ce qui pourrait venir de l'utilisateur : chaque passage entre
/// guillemets (« », " ", accents graves, ou ' ' ouvrant après une espace, pour épargner les
/// apostrophes de « d'accès ») devient `«…»`, un chemin (mot qui contient `/` ou `\`, avec
/// ses espaces éventuelles) devient `«chemin»`, et le tout est coupé à [`MAX_MESSAGE_CHARS`]
/// caractères.
pub fn sanitize(message: &str) -> String {
    let mut out = String::new();
    let mut chars = message.chars().peekable();
    let mut previous = ' ';
    while let Some(c) = chars.next() {
        let closing = match c {
            '«' => Some('»'),
            '"' => Some('"'),
            // Les messages de panique de la bibliothèque standard citent entre accents graves.
            '`' => Some('`'),
            '\'' if previous.is_whitespace() || previous == '(' => Some('\''),
            _ => None,
        };
        match closing {
            Some(end) if chars.clone().any(|n| n == end) => {
                for n in chars.by_ref() {
                    if n == end {
                        break;
                    }
                }
                out.push_str("«…»");
                previous = '»';
            }
            _ => {
                out.push(c);
                previous = c;
            }
        }
    }
    // Un chemin peut contenir des espaces (« C:\\Users\\Jean Dupont\\… ») : à partir du premier
    // mot qui contient un séparateur, on masque jusqu'à la parenthèse fermante si le chemin en
    // ouvre une, sinon jusqu'à la fin du message.
    let mut masked: Vec<String> = Vec::new();
    let mut words = out.split(' ');
    while let Some(word) = words.next() {
        if !word.contains(['/', '\\']) {
            masked.push(word.to_string());
            continue;
        }
        let core = word.trim_start_matches(['(', '[', '{']);
        let lead = &word[..word.len() - core.len()];
        if !lead.contains('(') {
            masked.push(format!("{lead}«chemin»"));
            break;
        }
        let mut last = word;
        while !last.contains(')') {
            match words.next() {
                Some(next) => last = next,
                None => break,
            }
        }
        let tail = last.find(')').map_or("", |k| &last[k..]);
        masked.push(format!("{lead}«chemin»{tail}"));
    }
    let text = masked.join(" ");
    if text.chars().count() > MAX_MESSAGE_CHARS {
        let mut cut: String = text.chars().take(MAX_MESSAGE_CHARS).collect();
        cut.push('…');
        cut
    } else {
        text
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// « AAAA-MM-JJ HH:MM:SS UTC » d'un instant Unix.
///
/// \[Apprentissage\] Conversion d'un nombre de jours depuis le 1er janvier 1970 en date du
/// calendrier grégorien (algorithme de Howard Hinnant, « days from civil » à l'envers) : on
/// compte en « ères » de 400 ans, qui se répètent exactement, avec des années commençant en
/// mars pour que le 29 février tombe à la fin. Quelques lignes d'arithmétique entière plutôt
/// qu'une dépendance de plus.
pub fn utc_timestamp(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let secs = unix_seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier vide propre au test, dans le dossier temporaire du système.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mega-diag-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn timestamps_follow_the_gregorian_calendar() {
        assert_eq!(utc_timestamp(0), "1970-01-01 00:00:00 UTC");
        // 29 février 2024, 12:34:56.
        assert_eq!(utc_timestamp(1_709_210_096), "2024-02-29 12:34:56 UTC");
        assert_eq!(utc_timestamp(1_798_675_200), "2026-12-31 00:00:00 UTC");
    }

    #[test]
    fn the_log_rotates_past_its_cap() {
        let dir = scratch("rotation");
        let log = DiagnosticLog::new(dir.join("journaux"), 200);
        for i in 0..20 {
            log.write(&format!("erreur numéro {i}"));
        }
        let current = std::fs::read_to_string(dir.join("journaux").join(LOG_NAME)).unwrap();
        let old = std::fs::read_to_string(dir.join("journaux").join(OLD_LOG_NAME)).unwrap();
        assert!(current.len() as u64 <= 200, "{current}");
        assert!(current.contains("erreur numéro 19"));
        assert!(!old.is_empty());
        assert!(current.lines().all(|l| l.contains(" UTC erreur numéro ")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_multiline_message_stays_on_one_line() {
        let dir = scratch("lignes");
        let log = DiagnosticLog::new(dir.clone(), MAX_LOG_BYTES);
        log.write("première\nseconde");
        let text = std::fs::read_to_string(dir.join(LOG_NAME)).unwrap();
        assert_eq!(text.lines().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Relecture de sécurité : un message d'erreur peut citer ce que l'utilisateur a saisi
    /// (un caractère de mot de passe, un texte refusé) ou un chemin ; le journal n'en garde
    /// que la forme.
    #[test]
    fn quoted_text_and_paths_are_masked() {
        assert_eq!(
            sanitize("le mot de passe contient un caractère non pris en charge : « é »"),
            "le mot de passe contient un caractère non pris en charge : «…»"
        );
        assert_eq!(
            sanitize(r#"texte "secret" refusé, police 'Perso'"#),
            "texte «…» refusé, police «…»"
        );
        assert_eq!(
            sanitize(r"Tesseract introuvable (C:\Users\moi\tesseract.exe) : échec"),
            "Tesseract introuvable («chemin») : échec"
        );
        assert_eq!(
            sanitize("lecture de /home/moi/Documents/lettre.pdf impossible"),
            // Fin du chemin inconnue (il peut contenir des espaces) : masqué jusqu'au bout.
            "lecture de «chemin»"
        );
        assert!(sanitize(&"x".repeat(1000)).chars().count() <= MAX_MESSAGE_CHARS + 1);
    }

    /// Relecture de la PR A : un message de panique de la bibliothèque standard cite la chaîne
    /// fautive entre accents graves (du texte de document, peut-être), et un chemin peut
    /// contenir des espaces.
    #[test]
    fn panic_quotes_and_paths_with_spaces_are_masked() {
        // Format des versions de Rust qui citent la chaîne fautive entre accents graves.
        let message = "byte index 23 is not a char boundary; it is inside 'é' (bytes 23..25)                        of `Facture confidentielle é`";
        assert!(
            !sanitize(message).contains("confidentielle"),
            "{}",
            sanitize(message)
        );
        assert_eq!(
            sanitize(r"le fichier C:\Users\Jean Dupont\Mes documents\Facture EDF.png existe déjà"),
            "le fichier «chemin»"
        );
        assert_eq!(
            sanitize(
                r"Tesseract introuvable (C:\Program Files\Tesseract-OCR\tesseract.exe) : échec"
            ),
            "Tesseract introuvable («chemin») : échec"
        );
    }

    #[test]
    fn password_prompts_are_not_logged() {
        assert!(!worth_logging(ErrorKind::PasswordRequired));
        assert!(!worth_logging(ErrorKind::WrongPassword));
        assert!(worth_logging(ErrorKind::Internal));
        assert!(worth_logging(ErrorKind::InvalidPdf));
    }
}
