// SPDX-License-Identifier: GPL-3.0-or-later
//! Définition de la ligne de commande avec `clap` (mode « derive »).
//!
//! \[Apprentissage\] `#[derive(Parser)]` génère tout l'analyseur d'arguments à partir
//! de la structure : noms des options, valeurs par défaut, aide `--help`. Les
//! commentaires `///` deviennent les textes d'aide.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

/// MegaUltraPDF en ligne de commande.
#[derive(Debug, Parser)]
#[command(
    name = "mega",
    version,
    about = "Outil en ligne de commande de MegaUltraPDF"
)]
pub struct Cli {
    /// Sous-commande choisie par l'utilisateur.
    #[command(subcommand)]
    pub command: Command,
    /// Écraser un fichier de sortie qui existe déjà (sans cette option, `mega` refuse, y
    /// compris quand la sortie est le fichier source).
    #[arg(long, global = true)]
    pub force: bool,
}

/// Les sous-commandes disponibles.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Affiche les informations d'un PDF (titre, nombre de pages, taille de la page 1).
    Info {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Rend une page en image PNG.
    Render {
        /// Fichier PDF source.
        file: PathBuf,
        /// Numéro de page (à partir de 1).
        #[arg(long, default_value_t = 1)]
        page: u16,
        /// Facteur d'échelle (1.0 = 72 points par pouce).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Fichier PNG à écrire.
        #[arg(long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Fusionne plusieurs PDF, dans l'ordre donné, en un nouveau fichier.
    Merge {
        /// Fichiers à fusionner (au moins un).
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Scinde un PDF en plusieurs fichiers `<préfixe>-N.pdf`.
    Split {
        /// Fichier PDF source.
        file: PathBuf,
        /// Plages explicites, une partie par plage (ex. « 1-2,3-5 »).
        #[arg(long, conflicts_with_all = ["every", "bookmarks"])]
        ranges: Option<String>,
        /// Une partie toutes les N pages (N ≥ 1).
        #[arg(long, conflicts_with = "bookmarks", value_parser = clap::value_parser!(u16).range(1..))]
        every: Option<u16>,
        /// Une partie par signet de premier niveau.
        #[arg(long)]
        bookmarks: bool,
        /// Dossier de sortie (créé si besoin).
        #[arg(short, long)]
        out: PathBuf,
        /// Préfixe des fichiers produits (défaut : nom du fichier source).
        #[arg(long)]
        prefix: Option<String>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Pivote des pages (rotation enregistrée dans le fichier) vers un nouveau fichier.
    Rotate {
        /// Fichier PDF source.
        file: PathBuf,
        /// Pages à pivoter (ex. « 1,3-4 »).
        #[arg(long)]
        pages: String,
        /// Angle horaire : 90, 180 ou 270.
        #[arg(long)]
        degrees: u16,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste les annotations d'un PDF.
    Annots {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Page (à partir de 1) ; toutes si absent.
        #[arg(long)]
        page: Option<u16>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste les champs de formulaire d'un PDF.
    Fields {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Aplatit formulaire et annotations dans un nouveau fichier.
    Flatten {
        /// Fichier PDF source.
        file: PathBuf,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Copie des pages dans un nouveau fichier.
    Extract {
        /// Fichier PDF source.
        file: PathBuf,
        /// Pages à extraire (ex. « 2-3 »), dans l'ordre donné.
        #[arg(long)]
        pages: String,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste les objets du contenu (textes, images, tracés) d'un PDF.
    Objects {
        /// Fichier PDF à inspecter.
        file: PathBuf,
        /// Page (à partir de 1) ; toutes si absent.
        #[arg(long)]
        page: Option<u16>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Caviarde une zone d'une page (le contenu disparaît du fichier) vers un nouveau fichier.
    Redact {
        /// Fichier PDF source.
        file: PathBuf,
        /// Pages (ex. « 2 » ou « 1-3 », à partir de 1) ; `--page` est accepté.
        #[arg(long, alias = "page")]
        pages: String,
        /// Zone « x,y,w,h » en points, origine en bas à gauche de la page affichée.
        #[arg(long)]
        rect: String,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Pose un filigrane texte en diagonale vers un nouveau fichier.
    Watermark {
        /// Fichier PDF source.
        file: PathBuf,
        /// Le texte du filigrane.
        #[arg(long)]
        text: String,
        /// Taille de police en points.
        #[arg(long, default_value_t = 60.0)]
        size: f32,
        /// Opacité, de 0 (exclu) à 1.
        #[arg(long, default_value_t = 0.3)]
        opacity: f32,
        /// Inclinaison en degrés.
        #[arg(long, default_value_t = 45.0)]
        angle: f32,
        /// Pages visées (ex. « 1,3-4 ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Numérote les pages (modèle `{n}`, `{total}`, `{titre}`) vers un nouveau fichier.
    Number {
        /// Fichier PDF source.
        file: PathBuf,
        /// Modèle du texte.
        #[arg(long, default_value = "{n} / {total}")]
        template: String,
        /// Position : top_left, top_center, top_right, bottom_left, bottom_center, bottom_right.
        #[arg(long, default_value = "bottom_center")]
        corner: String,
        /// Marge depuis les bords, en points.
        #[arg(long, default_value_t = 28.0)]
        margin: f32,
        /// Taille de police en points.
        #[arg(long, default_value_t = 10.0)]
        size: f32,
        /// Numéro de la première page.
        #[arg(long, default_value_t = 1)]
        start: u16,
        /// Pages visées (ex. « 2-10 ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Exporte des pages en images PNG (ou JPEG avec --jpeg) dans un dossier.
    ExportImages {
        /// Fichier PDF source.
        file: PathBuf,
        /// Dossier de sortie (créé si besoin).
        #[arg(short, long)]
        out: PathBuf,
        /// Pages (ex. « 1-3 ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Résolution en points par pouce (36 à 600).
        #[arg(long, default_value_t = 150)]
        dpi: u16,
        /// Produire du JPEG plutôt que du PNG.
        #[arg(long)]
        jpeg: bool,
        /// Qualité JPEG (10 à 100).
        #[arg(long, default_value_t = 85)]
        quality: u8,
        /// Préfixe des fichiers (défaut : nom du fichier source).
        #[arg(long)]
        prefix: Option<String>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Exporte le texte des pages dans un fichier UTF-8.
    ExportText {
        /// Fichier PDF source.
        file: PathBuf,
        /// Fichier texte à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Pages (ex. « 2- ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Ne pas écrire de ligne « ===== Page N ===== » entre les pages.
        #[arg(long)]
        no_separators: bool,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Affiche les métadonnées et la sécurité ; avec --title/--author/…, les écrit.
    Meta {
        /// Fichier PDF.
        file: PathBuf,
        /// Nouveau titre (vide pour retirer).
        #[arg(long)]
        title: Option<String>,
        /// Nouvel auteur.
        #[arg(long)]
        author: Option<String>,
        /// Nouveau sujet.
        #[arg(long)]
        subject: Option<String>,
        /// Nouveaux mots-clés.
        #[arg(long)]
        keywords: Option<String>,
        /// Fichier PDF à écrire (défaut : le fichier lui-même).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Protège un PDF par mot de passe (AES-256) vers un nouveau fichier.
    Protect {
        /// Fichier PDF source.
        file: PathBuf,
        /// Mot de passe d'ouverture (vide : aucun).
        #[arg(long, default_value = "")]
        user: String,
        /// Mot de passe propriétaire (vide : identique au mot de passe d'ouverture).
        #[arg(long, default_value = "")]
        owner: String,
        /// Interdire l'impression.
        #[arg(long)]
        no_print: bool,
        /// Interdire la copie du texte et des images.
        #[arg(long)]
        no_copy: bool,
        /// Interdire la modification.
        #[arg(long)]
        no_modify: bool,
        /// Interdire les annotations et le remplissage de formulaires.
        #[arg(long)]
        no_annotate: bool,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe du fichier source, s'il est déjà chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Retire la protection d'un PDF ouvert avec son mot de passe, vers un nouveau fichier.
    Unprotect {
        /// Fichier PDF source.
        file: PathBuf,
        /// Mot de passe (utilisateur ou propriétaire).
        #[arg(long)]
        password: Option<String>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Optimise un PDF (élagage, compression, images) vers un nouveau fichier.
    Optimize {
        /// Fichier PDF source.
        file: PathBuf,
        /// Qualité JPEG des images recomprimées (10 à 100).
        #[arg(long, default_value_t = 75)]
        quality: u8,
        /// Dimension maximale des images en pixels (256 à 8192) ; aucune réduction si absent.
        #[arg(long)]
        max_px: Option<u32>,
        /// Ne pas compresser les flux non compressés.
        #[arg(long)]
        no_compress: bool,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste les pièces jointes d'un PDF, ou en extrait une.
    Attachments {
        /// Fichier PDF.
        file: PathBuf,
        /// Index de la pièce jointe à extraire (voir la liste).
        #[arg(long)]
        extract: Option<u16>,
        /// Fichier à écrire (avec --extract).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Compare une page de deux PDF et écrit l'image des différences (PNG).
    Compare {
        /// Premier fichier.
        a: PathBuf,
        /// Second fichier.
        b: PathBuf,
        /// Page du premier fichier (à partir de 1).
        #[arg(long, default_value_t = 1)]
        page: u16,
        /// Page du second fichier (défaut : la même).
        #[arg(long)]
        page_b: Option<u16>,
        /// Échelle de rendu (1 = 72 dpi, au plus 4).
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Image PNG à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour des PDF chiffrés.
        #[arg(long)]
        password: Option<String>,
        /// Mot de passe du second fichier (défaut : celui de `--password`).
        #[arg(long)]
        password_b: Option<String>,
    },
    /// Recadre des pages par marges « gauche,bas,droite,haut » (0,0,0,0 réinitialise).
    Crop {
        /// Fichier PDF source.
        file: PathBuf,
        /// Marges en points, depuis la page entière.
        #[arg(long)]
        margins: String,
        /// Pages visées (ex. « 1 ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Rend cherchables les pages numérisées : reconnaissance du texte par Tesseract, ajouté
    /// en couche invisible (les pages qui ont déjà du texte sont ignorées).
    #[cfg(feature = "ocr")]
    Ocr {
        /// Fichier PDF source.
        file: PathBuf,
        /// Langues Tesseract, séparées par `+` (ex. « fra+eng »).
        #[arg(long, default_value = "eng")]
        lang: String,
        /// Pages visées (ex. « 1,3-4 ») ; toutes si absent.
        #[arg(long)]
        pages: Option<String>,
        /// Chemin de l'exécutable `tesseract` (sinon : MEGA_TESSERACT, PATH, dossiers usuels).
        #[arg(long)]
        tesseract: Option<PathBuf>,
        /// Fichier PDF à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Signe un PDF avec un certificat PKCS#12 (.p12 / .pfx) ; le mot de passe du certificat
    /// est lu dans MEGA_P12_PASSWORD ou sur l'entrée standard.
    #[cfg(feature = "signature")]
    Sign {
        /// Fichier PDF source.
        file: PathBuf,
        /// Fichier du certificat (PKCS#12).
        #[arg(long)]
        p12: PathBuf,
        /// Motif de la signature.
        #[arg(long)]
        reason: Option<String>,
        /// Lieu de la signature.
        #[arg(long)]
        location: Option<String>,
        /// Contact du signataire.
        #[arg(long)]
        contact: Option<String>,
        /// Fichier PDF signé à écrire.
        #[arg(short, long)]
        out: PathBuf,
        /// Mot de passe du PDF, s'il est chiffré.
        #[arg(long)]
        password: Option<String>,
    },
    /// Liste et vérifie les signatures numériques d'un PDF.
    #[cfg(feature = "signature")]
    Signatures {
        /// Fichier PDF à vérifier.
        file: PathBuf,
        /// Mot de passe, pour un PDF chiffré.
        #[arg(long)]
        password: Option<String>,
    },
}

impl Command {
    /// Le fichier unique que la commande va écrire, s'il y en a un : `main` refuse de
    /// l'écraser sans `--force`. (`split` et `export-images` écrivent dans un dossier et
    /// contrôlent eux-mêmes chaque fichier produit ; `meta` sans `--out` modifie le fichier
    /// en place, ce qui est son usage voulu.)
    pub fn output_file(&self) -> Option<&Path> {
        match self {
            Command::Render { out, .. }
            | Command::Merge { out, .. }
            | Command::Rotate { out, .. }
            | Command::Flatten { out, .. }
            | Command::Extract { out, .. }
            | Command::Redact { out, .. }
            | Command::Watermark { out, .. }
            | Command::Number { out, .. }
            | Command::ExportText { out, .. }
            | Command::Protect { out, .. }
            | Command::Unprotect { out, .. }
            | Command::Optimize { out, .. }
            | Command::Compare { out, .. }
            | Command::Crop { out, .. } => Some(out),
            Command::Meta { out, .. } | Command::Attachments { out, .. } => out.as_deref(),
            #[cfg(feature = "ocr")]
            Command::Ocr { out, .. } => Some(out),
            #[cfg(feature = "signature")]
            Command::Sign { out, .. } => Some(out),
            _ => None,
        }
    }

    /// Le PDF source que la commande **réécrit** (sans mise à jour incrémentale) : ses
    /// signatures numériques ne seront plus valides dans le fichier produit.
    pub fn rewritten_input(&self) -> Option<&Path> {
        match self {
            Command::Rotate { file, .. }
            | Command::Flatten { file, .. }
            | Command::Extract { file, .. }
            | Command::Redact { file, .. }
            | Command::Watermark { file, .. }
            | Command::Number { file, .. }
            | Command::Protect { file, .. }
            | Command::Unprotect { file, .. }
            | Command::Optimize { file, .. }
            | Command::Crop { file, .. } => Some(file),
            Command::Meta {
                file,
                title,
                author,
                subject,
                keywords,
                ..
            } if title.is_some() || author.is_some() || subject.is_some() || keywords.is_some() => {
                Some(file)
            }
            #[cfg(feature = "ocr")]
            Command::Ocr { file, .. } => Some(file),
            _ => None,
        }
    }
}
