// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega meta`, `mega protect`, `mega unprotect` : métadonnées et protection par mot de passe.

use std::path::Path;

use anyhow::Context;
use mega_core::properties::MetadataPatch;
use mega_core::security::{Permissions, Protection};
use mega_core::{Document, EditCommand};

fn oui_non(b: bool) -> &'static str {
    if b { "oui" } else { "non" }
}

/// Affiche les métadonnées et l'état de sécurité ; avec au moins un `--champ`, les écrit
/// dans `out` (ou dans le fichier lui-même si `out` est absent).
#[allow(clippy::too_many_arguments)]
pub fn meta(
    file: &Path,
    title: Option<&str>,
    author: Option<&str>,
    subject: Option<&str>,
    keywords: Option<&str>,
    out: Option<&Path>,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let mut doc = Document::open(file, password)?;
    let patch = MetadataPatch {
        title: title.map(str::to_string),
        author: author.map(str::to_string),
        subject: subject.map(str::to_string),
        keywords: keywords.map(str::to_string),
    };
    if patch != MetadataPatch::default() {
        doc.apply(&EditCommand::SetMetadata { patch })?;
        let target = out.unwrap_or(file);
        doc.save_as(target)
            .with_context(|| format!("écriture de {}", target.display()))?;
        println!("Métadonnées écrites : {}", target.display());
    }
    let p = doc.properties()?;
    let safe = |v: &Option<String>| super::info::display_safe(v.as_deref().unwrap_or("—"));
    println!("Titre : {}", safe(&p.title));
    println!("Auteur : {}", safe(&p.author));
    println!("Sujet : {}", safe(&p.subject));
    println!("Mots-clés : {}", safe(&p.keywords));
    println!("Créé le : {}", safe(&p.creation_date));
    println!("Modifié le : {}", safe(&p.modification_date));
    let s = doc.security()?;
    println!("Protégé : {}", oui_non(s.encrypted));
    if s.encrypted {
        println!(
            "Permissions : impression : {}, copie : {}, modification : {}, annotation : {}",
            oui_non(s.permissions.print),
            oui_non(s.permissions.copy),
            oui_non(s.permissions.modify),
            oui_non(s.permissions.annotate)
        );
    }
    Ok(())
}

/// Protège `file` (AES-256) et écrit `out`.
#[allow(clippy::too_many_arguments)]
pub fn protect(
    file: &Path,
    user: &str,
    owner: &str,
    no_print: bool,
    no_copy: bool,
    no_modify: bool,
    no_annotate: bool,
    out: &Path,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let mut doc = Document::open(file, password)?;
    doc.apply(&EditCommand::Protect {
        protection: Protection {
            user_password: user.to_string(),
            owner_password: owner.to_string(),
            permissions: Permissions {
                print: !no_print,
                copy: !no_copy,
                modify: !no_modify,
                annotate: !no_annotate,
            },
        },
    })?;
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!("Document protégé (AES-256) : {}", out.display());
    Ok(())
}

/// Retire la protection de `file` ouvert avec `password` et écrit `out`.
pub fn unprotect(file: &Path, password: Option<&str>, out: &Path) -> anyhow::Result<()> {
    let mut doc = Document::open(file, password)?;
    doc.apply(&EditCommand::Unprotect)?;
    doc.save_as(out)
        .with_context(|| format!("écriture de {}", out.display()))?;
    println!("Protection retirée : {}", out.display());
    Ok(())
}
