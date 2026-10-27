// SPDX-License-Identifier: GPL-3.0-or-later
//! `mega sign` et `mega signatures` (feature `signature`, Lot 8).

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::Context;
use mega_core::Document;
use mega_core::digital_signature::{SignOptions, SignatureInfo, SignatureStatus, SigningIdentity};

use super::info::display_safe;

/// Variable d'environnement qui fournit le mot de passe du fichier PKCS#12 (sinon il est lu
/// sur l'entrée standard, jamais passé en argument : il resterait dans l'historique du shell).
pub const P12_PASSWORD_ENV: &str = "MEGA_P12_PASSWORD";

/// Mot de passe du certificat : variable d'environnement, sinon une ligne de l'entrée standard.
fn p12_password() -> anyhow::Result<String> {
    if let Ok(p) = std::env::var(P12_PASSWORD_ENV) {
        return Ok(p);
    }
    // Au clavier : saisie **sans écho** (le mot de passe ne s'affiche pas à l'écran). Depuis
    // un tube (`echo … | mega sign`), une simple ligne lue.
    if std::io::stdin().is_terminal() {
        return rpassword::prompt_password("Mot de passe du certificat : ")
            .context("lecture du mot de passe");
    }
    eprintln!("Mot de passe du certificat :");
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("lecture du mot de passe")?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Signe `file` avec le certificat `p12` et écrit `out`.
pub fn sign(
    file: &Path,
    p12: &Path,
    out: &Path,
    options: SignOptions,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let bytes = std::fs::read(p12).with_context(|| format!("lecture de {}", p12.display()))?;
    let identity = SigningIdentity::from_pkcs12(&bytes, &p12_password()?)?;
    let mut doc = Document::open(file, password)?;
    doc.sign_to(out, &identity, &options)
        .with_context(|| format!("signature de {}", file.display()))?;
    println!(
        "Document signé par « {} » : {}",
        display_safe(&identity.signer_name()),
        out.display()
    );
    Ok(())
}

/// Verdict lisible d'une signature.
fn verdict(info: &SignatureInfo) -> String {
    match &info.status {
        SignatureStatus::Intact if info.covers_whole_document => {
            "intègre, couvre tout le document (identité non certifiée)".to_string()
        }
        SignatureStatus::Intact => {
            "intègre, mais le document a été modifié après cette signature".to_string()
        }
        SignatureStatus::Invalid(why) => format!("INVALIDE : {why}"),
        SignatureStatus::Unsupported(why) => format!("non vérifiable : {why}"),
    }
}

/// Liste les signatures de `file`, vérifiées.
pub fn list(file: &Path, password: Option<&str>) -> anyhow::Result<()> {
    let doc = Document::open(file, password)?;
    let list = doc.signatures()?;
    if list.is_empty() {
        println!("Aucune signature numérique.");
        return Ok(());
    }
    for info in &list {
        println!(
            "Signature « {} » : {}",
            display_safe(&info.field),
            verdict(info)
        );
        if let Some(signer) = &info.signer {
            println!("  Signataire : {}", display_safe(signer));
        }
        if let Some(when) = info
            .signed_at
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        {
            println!("  Date : {}", when.format("%Y-%m-%d %H:%M:%S UTC"));
        }
        if let Some(reason) = &info.reason {
            println!("  Motif : {}", display_safe(reason));
        }
        if let Some(location) = &info.location {
            println!("  Lieu : {}", display_safe(location));
        }
        println!("  Format : {}", display_safe(&info.sub_filter));
    }
    Ok(())
}
