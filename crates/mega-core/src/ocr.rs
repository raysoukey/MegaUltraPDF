// SPDX-License-Identifier: GPL-3.0-or-later
//! OCR (feature `ocr`, Lot 8) : la commande externe `tesseract` reconnaît le texte d'une page
//! rendue en image, et ses mots sont ajoutés en **couche de texte invisible**
//! ([`Document::add_invisible_words`]) : le rendu ne change pas, la recherche et la copie
//! trouvent le texte.
//!
//! Aucun moteur n'est lié ni embarqué : Tesseract est installé par l'utilisateur et lancé
//! comme processus (`std::process::Command`, liste d'arguments, jamais de shell). Rien ne
//! passe par le réseau.
//!
//! \[Apprentissage\] Ce module entier n'existe que si la feature `ocr` est activée
//! (`#[cfg(feature = "ocr")] pub mod ocr;` dans `lib.rs`) : sans elle, son code n'est même
//! pas compilé. Voir la fiche `features-cargo-et-dependances`.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::document::Document;
use crate::error::{CoreError, CoreResult};
use crate::geometry::Rect;
use crate::overlay::InvisibleWord;

/// Résolution du rendu envoyé à Tesseract : 300 dpi, sa résolution de travail recommandée.
pub const OCR_DPI: f32 = 300.0;

/// Délai accordé à Tesseract pour reconnaître **une** page (5 minutes ; une page A4 à 300 dpi
/// prend quelques secondes). Au-delà, le processus est tué : un Tesseract bloqué figerait
/// sinon l'acteur PDFium, donc toute l'application (audit du Lot 13, R2).
pub const OCR_TIMEOUT: Duration = Duration::from_secs(300);

/// Délai accordé à `tesseract --list-langs`.
const LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// Lance `cmd` et attend sa fin au plus `timeout` ; `Ok(None)` si le délai est dépassé (le
/// processus est alors tué).
///
/// \[Apprentissage\] **Lire pendant qu'on attend** : un processus qui écrit plus que le tampon
/// d'un tube (64 Ko environ) reste bloqué tant que personne ne lit. On lit donc sa sortie
/// standard et sa sortie d'erreur dans deux threads (`std::thread::spawn`), pendant que le
/// thread courant surveille la fin du processus avec `try_wait` (qui ne bloque pas). Sans
/// délai, `Command::output()` attendrait pour toujours un programme bloqué.
fn output_within(cmd: &mut Command, timeout: Duration) -> std::io::Result<Option<Output>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = pipe {
                // Une erreur de lecture laisse simplement une sortie incomplète.
                let _ = pipe.read_to_end(&mut buffer);
            }
            buffer
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            // Le processus a pu finir entre-temps : l'échec de `kill` est sans conséquence.
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(status.map(|status| Output {
        status,
        stdout,
        stderr,
    }))
}

/// Variable d'environnement qui désigne l'exécutable `tesseract`.
pub const TESSERACT_ENV: &str = "MEGA_TESSERACT";

/// Chemin réglé par l'application (préférences), prioritaire sur tout le reste.
static APP_TESSERACT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Langues installées du dernier Tesseract interrogé : `--list-langs` lancé une fois, pas à
/// chaque page (C40). Rafraîchi par [`ocr_status`] (ouverture du dialogue) et vidé quand le
/// chemin de Tesseract change.
static LANGUAGES: RwLock<Option<(PathBuf, Vec<String>)>> = RwLock::new(None);

/// Langues de `exe`, depuis le cache si c'est le même exécutable.
fn installed_languages(exe: &Path) -> CoreResult<Vec<String>> {
    if let Ok(cache) = LANGUAGES.read()
        && let Some((cached, languages)) = cache.as_ref()
        && cached == exe
    {
        return Ok(languages.clone());
    }
    let languages = tesseract_languages(exe)?;
    if let Ok(mut cache) = LANGUAGES.write() {
        *cache = Some((exe.to_path_buf(), languages.clone()));
    }
    Ok(languages)
}

/// Un mot reconnu, avec sa boîte en **pixels** de l'image analysée (origine en haut à gauche).
#[derive(Debug, Clone, PartialEq)]
pub struct OcrWord {
    /// Le texte du mot.
    pub text: String,
    /// Bord gauche, en pixels.
    pub left: u32,
    /// Bord haut, en pixels.
    pub top: u32,
    /// Largeur, en pixels.
    pub width: u32,
    /// Hauteur, en pixels.
    pub height: u32,
    /// Confiance de Tesseract, de 0 à 100.
    pub confidence: f32,
}

/// Une ligne de texte reconnue : sa boîte (pixels) et ses mots, dans l'ordre de lecture.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    /// Bord gauche, en pixels.
    pub left: u32,
    /// Bord haut, en pixels.
    pub top: u32,
    /// Largeur, en pixels.
    pub width: u32,
    /// Hauteur, en pixels.
    pub height: u32,
    /// Les mots de la ligne.
    pub words: Vec<OcrWord>,
}

/// Lit la sortie `tsv` de Tesseract : les lignes (niveau 4) et leurs mots (niveau 5). Les
/// mots vides ou de confiance négative sont ignorés, les lignes sans mot aussi ; une ligne
/// mal formée est sautée (la sortie d'un programme externe est une donnée non fiable).
pub fn parse_tsv(tsv: &str) -> Vec<OcrLine> {
    let mut lines: Vec<OcrLine> = Vec::new();
    for row in tsv.lines().skip(1) {
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let num = |i: usize| cols[i].trim().parse::<u32>().ok();
        let (Some(level), Some(left), Some(top), Some(width), Some(height)) =
            (num(0), num(6), num(7), num(8), num(9))
        else {
            continue;
        };
        match level {
            4 => lines.push(OcrLine {
                left,
                top,
                width,
                height,
                words: Vec::new(),
            }),
            5 => {
                let text = cols[11..].join("\t").trim().to_string();
                let confidence = cols[10].trim().parse::<f32>().unwrap_or(-1.0);
                if text.is_empty() || confidence < 0.0 {
                    continue;
                }
                if let Some(line) = lines.last_mut() {
                    line.words.push(OcrWord {
                        text,
                        left,
                        top,
                        width,
                        height,
                        confidence,
                    });
                }
            }
            _ => {}
        }
    }
    lines.retain(|l| !l.words.is_empty());
    lines
}

/// Passe des lignes reconnues (pixels d'une image `image_w` × `image_h`) aux mots de la
/// couche invisible, en points de la page affichée `page_w` × `page_h` (axe y vers le haut).
///
/// Chaque mot garde sa position et sa largeur, mais prend la hauteur et le haut de **sa
/// ligne** : tous les mots d'une ligne partagent ainsi taille de police et ligne de base. Une
/// espace est ajoutée après chaque mot sauf le dernier de la ligne, pour que la recherche
/// trouve « Page 2 » d'un seul tenant.
pub fn lines_to_words(
    lines: &[OcrLine],
    image_w: u32,
    image_h: u32,
    page_w: f32,
    page_h: f32,
) -> Vec<InvisibleWord> {
    if image_w == 0 || image_h == 0 {
        return Vec::new();
    }
    let sx = page_w / image_w as f32;
    let sy = page_h / image_h as f32;
    let mut out = Vec::new();
    for line in lines {
        let h = line.height as f32 * sy;
        let y = page_h - (line.top as f32 + line.height as f32) * sy;
        let last = line.words.len().saturating_sub(1);
        for (i, w) in line.words.iter().enumerate() {
            let text = if i < last {
                format!("{} ", w.text)
            } else {
                w.text.clone()
            };
            out.push(InvisibleWord {
                text,
                rect: Rect {
                    x: w.left as f32 * sx,
                    y,
                    w: w.width as f32 * sx,
                    h,
                },
            });
        }
    }
    out
}

/// Nom de l'exécutable selon le système.
fn exe_name() -> &'static str {
    if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    }
}

/// Candidats, dans l'ordre : chemin réglé par l'application, variable `MEGA_TESSERACT`,
/// chaque dossier du `PATH`, puis les dossiers d'installation usuels. Fonction pure (testable).
pub fn search_candidates(
    app: Option<&Path>,
    env: Option<OsString>,
    path_var: Option<OsString>,
) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = Vec::new();
    list.extend(app.map(Path::to_path_buf));
    list.extend(env.map(PathBuf::from));
    if let Some(path_var) = path_var {
        // Une entrée vide du `PATH` désignerait le dossier courant : un `tesseract.exe` posé
        // à côté d'un document ouvert serait lancé. Seuls les dossiers absolus comptent (C41).
        list.extend(
            std::env::split_paths(&path_var)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(exe_name())),
        );
    }
    let usual: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\Tesseract-OCR",
            r"C:\Program Files (x86)\Tesseract-OCR",
        ]
    } else {
        &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
    };
    list.extend(usual.iter().map(|dir| Path::new(dir).join(exe_name())));
    list
}

/// Règle le chemin de `tesseract` choisi dans les préférences (`None` : recherche normale).
pub fn set_tesseract_path(path: Option<PathBuf>) {
    if let Ok(mut slot) = APP_TESSERACT.write() {
        *slot = path;
    }
    if let Ok(mut cache) = LANGUAGES.write() {
        *cache = None;
    }
}

/// Le premier candidat qui existe, ou `None` si Tesseract n'est pas installé.
pub fn find_tesseract() -> Option<PathBuf> {
    let app = APP_TESSERACT.read().ok().and_then(|slot| slot.clone());
    search_candidates(
        app.as_deref(),
        std::env::var_os(TESSERACT_ENV),
        std::env::var_os("PATH"),
    )
    .into_iter()
    .find(|p| p.is_file())
}

/// Une commande `tesseract` prête à lancer, sans fenêtre de console sous Windows.
fn tesseract_command(exe: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        // \[Apprentissage\] `CREATE_NO_WINDOW` : l'application graphique ne fait pas
        // clignoter une console à chaque page. Extension propre à Windows, d'où le `cfg`.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn unavailable(exe: &Path, detail: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidOperation(format!(
        "Tesseract introuvable ou inutilisable ({}) : {detail}",
        exe.display()
    ))
}

/// Langues installées pour Tesseract (`--list-langs`), sans `osd` (détection
/// d'orientation, pas une langue).
pub fn tesseract_languages(exe: &Path) -> CoreResult<Vec<String>> {
    let out = output_within(tesseract_command(exe).arg("--list-langs"), LIST_TIMEOUT)
        .map_err(|e| unavailable(exe, e))?
        .ok_or_else(|| CoreError::Timeout {
            operation: "Liste des langues de Tesseract".to_string(),
            seconds: LIST_TIMEOUT.as_secs(),
        })?;
    if !out.status.success() {
        return Err(unavailable(
            exe,
            String::from_utf8_lossy(&out.stderr).trim(),
        ));
    }
    // Selon la version, la liste sort sur stdout ou stderr, après une ligne d'en-tête.
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut langs: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && l.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .filter(|l| *l != "osd")
        .map(str::to_string)
        .collect();
    langs.sort();
    langs.dedup();
    Ok(langs)
}

/// État de l'OCR pour l'interface : Tesseract trouvé ou non, et ses langues installées.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OcrStatus {
    /// Vrai si un exécutable `tesseract` utilisable a été trouvé.
    pub available: bool,
    /// Son chemin, s'il a été trouvé.
    pub path: Option<String>,
    /// Langues installées (sans `osd`), triées ; vide si Tesseract est absent.
    pub languages: Vec<String>,
}

/// L'état de l'OCR : où est Tesseract, et quelles langues il connaît. Un Tesseract trouvé
/// mais inutilisable (`--list-langs` échoue) compte comme absent.
pub fn ocr_status() -> OcrStatus {
    match find_tesseract().and_then(|exe| tesseract_languages(&exe).ok().map(|l| (exe, l))) {
        Some((exe, languages)) => {
            if let Ok(mut cache) = LANGUAGES.write() {
                *cache = Some((exe.clone(), languages.clone()));
            }
            OcrStatus {
                available: true,
                path: Some(exe.to_string_lossy().into_owned()),
                languages,
            }
        }
        None => OcrStatus {
            available: false,
            path: None,
            languages: Vec::new(),
        },
    }
}

impl Document {
    /// Vrai si la page contient du texte extractible (visible ou invisible) : une page déjà
    /// reconnue n'est pas reconnue une seconde fois.
    pub fn page_has_text(&self, page: u16) -> CoreResult<bool> {
        Ok(self.page_text(page)?.chars().any(|c| !c.is_whitespace()))
    }

    /// Reconnaît le texte de `page` avec le Tesseract trouvé par [`find_tesseract`].
    pub fn ocr_page(&mut self, page: u16, languages: &[String]) -> CoreResult<()> {
        let exe = find_tesseract().ok_or_else(|| {
            CoreError::InvalidOperation(
                "Tesseract introuvable : installez-le (avec ses langues) pour reconnaître le texte"
                    .to_string(),
            )
        })?;
        self.ocr_page_with(page, languages, &exe)
    }

    /// Reconnaît le texte de `page` avec l'exécutable `exe` : rendu à 300 dpi, Tesseract en
    /// TSV, puis couche de texte invisible. Refuse une page qui a déjà du texte et une langue
    /// non installée ; en cas d'erreur, le document n'a pas changé.
    pub fn ocr_page_with(&mut self, page: u16, languages: &[String], exe: &Path) -> CoreResult<()> {
        // Pas de `mark_changed` ici : en cas d'échec, rien n'a changé ;
        // `add_invisible_words` le fait au moment d'écrire.
        let count = self.page_count();
        if page >= count {
            return Err(CoreError::PageOutOfRange { index: page, count });
        }
        if languages.is_empty() {
            return Err(CoreError::InvalidOperation(
                "choisissez au moins une langue de reconnaissance".to_string(),
            ));
        }
        let installed = installed_languages(exe)?;
        if let Some(missing) = languages.iter().find(|l| !installed.contains(l)) {
            return Err(CoreError::InvalidOperation(format!(
                "langue de reconnaissance non installée pour Tesseract : {missing}"
            )));
        }
        if self.page_has_text(page)? {
            return Err(CoreError::InvalidOperation(format!(
                "la page {} contient déjà du texte",
                u32::from(page) + 1
            )));
        }
        let size = self.page_size(page)?;
        // Sans les annotations : le texte d'une note posée sur le scan n'appartient pas à la
        // page et n'entre pas dans sa couche de texte (C40).
        let image =
            self.render_page_opts(page, OCR_DPI / 72.0, crate::geometry::Rotation::None, false)?;
        let png = tempfile::Builder::new()
            .prefix("mega-ocr-")
            .suffix(".png")
            .tempfile()?;
        image::save_buffer(
            png.path(),
            &image.rgba,
            image.width_px,
            image.height_px,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| CoreError::InvalidOperation(format!("image pour l'OCR : {e}")))?;
        let mut cmd = tesseract_command(exe);
        cmd.arg(png.path())
            .arg("stdout")
            .arg("-l")
            .arg(languages.join("+"))
            .arg("--dpi")
            .arg(format!("{OCR_DPI}"))
            .arg("tsv");
        let out = output_within(&mut cmd, OCR_TIMEOUT)
            .map_err(|e| unavailable(exe, e))?
            .ok_or_else(|| CoreError::Timeout {
                operation: format!("Reconnaissance de texte (page {})", u32::from(page) + 1),
                seconds: OCR_TIMEOUT.as_secs(),
            })?;
        if !out.status.success() {
            return Err(unavailable(
                exe,
                String::from_utf8_lossy(&out.stderr).trim(),
            ));
        }
        let lines = parse_tsv(&String::from_utf8_lossy(&out.stdout));
        let words = lines_to_words(
            &lines,
            image.width_px,
            image.height_px,
            size.width_pt,
            size.height_pt,
        );
        self.add_invisible_words(page, &words)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Une commande qui dure environ `seconds` secondes sans rien écrire.
    fn sleeper(seconds: u32) -> Command {
        if cfg!(windows) {
            let mut cmd = Command::new("ping");
            cmd.args(["-n", &(seconds + 1).to_string(), "127.0.0.1"]);
            cmd
        } else {
            let mut cmd = Command::new("sleep");
            cmd.arg(seconds.to_string());
            cmd
        }
    }

    /// Une commande qui écrit `bytes` octets sur sa sortie (plus que le tampon d'un tube).
    fn talker(bytes: usize) -> Command {
        if cfg!(windows) {
            let mut cmd = Command::new("powershell");
            cmd.args(["-NoProfile", "-Command", &format!("'x' * {bytes}")]);
            cmd
        } else {
            let mut cmd = Command::new("head");
            cmd.args(["-c", &bytes.to_string(), "/dev/zero"]);
            cmd
        }
    }

    /// R2 (audit du Lot 13) : un processus qui ne rend pas la main est tué au délai, au lieu
    /// de figer l'acteur PDFium (et toute l'application) indéfiniment.
    #[test]
    fn a_process_that_hangs_is_killed_at_the_deadline() {
        let start = std::time::Instant::now();
        let out = output_within(&mut sleeper(30), std::time::Duration::from_millis(500)).unwrap();
        assert!(out.is_none(), "délai dépassé : pas de sortie");
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    /// Une sortie plus grande que le tampon d'un tube (64 Ko) ne bloque pas le processus.
    #[test]
    fn a_large_output_is_read_while_waiting() {
        let out = output_within(&mut talker(300_000), std::time::Duration::from_secs(60))
            .unwrap()
            .expect("terminé à temps");
        assert!(out.status.success());
        assert!(out.stdout.len() >= 300_000);
    }
}
