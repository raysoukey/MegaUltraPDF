// SPDX-License-Identifier: GPL-3.0-or-later
//! Le protocole `mega://` : sert les pages rendues en PNG au frontend.
//!
//! Le frontend affiche `<img src="…/page/<id>/<index>?scale=2">`. Plutôt que de faire
//! transiter l'image par `invoke` (JSON, base64), on enregistre un protocole
//! personnalisé : le WebView adresse la requête au Rust, qui répond avec les octets PNG,
//! comme un serveur HTTP local mais sans réseau. Sous Windows, le WebView voit ce
//! protocole sous la forme `http://mega.localhost/…`.
//!
//! `convertFileSrc` (côté TS) encode le chemin en pourcentage (`page%2F3%2F0`) ; on le
//! décode donc avant de l'analyser.

use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext};

use crate::commands::{SharedRegistry, lock_registry};
use crate::registry::DocId;

/// Nom du protocole, tel qu'enregistré auprès de Tauri et utilisé dans la CSP.
pub const SCHEME: &str = "mega";

const MIN_SCALE: f32 = 0.1;
const MAX_SCALE: f32 = 8.0;

/// Une demande de page analysée depuis l'URL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRequest {
    /// Identifiant du document dans le registre.
    pub id: DocId,
    /// Index de page, à partir de 0.
    pub index: u16,
    /// Échelle de rendu, déjà bornée à [`MIN_SCALE`, `MAX_SCALE`].
    pub scale: f32,
}

/// Pourquoi une URL n'est pas une demande de page valide.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ProtocolError {
    /// Le chemin ne commence pas par `page/<id>/<index>`.
    #[error("route inconnue : {0}")]
    UnknownRoute(String),
    /// `id`, `index` ou `scale` n'est pas un nombre valide.
    #[error("paramètre invalide : {0}")]
    BadParameter(&'static str),
}

/// Analyse `page/<id>/<index>` (éventuellement percent-encodé) et `scale=<f32>`.
///
/// L'échelle est bornée à [0,1 ; 8] pour qu'une URL forgée ne demande pas une image
/// de plusieurs gigaoctets.
pub fn parse_page_request(path: &str, query: Option<&str>) -> Result<PageRequest, ProtocolError> {
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    let mut parts = decoded.trim_start_matches('/').split('/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("page"), Some(id), Some(index), None) => {
            let id = id
                .parse::<DocId>()
                .map_err(|_| ProtocolError::BadParameter("id"))?;
            let index = index
                .parse::<u16>()
                .map_err(|_| ProtocolError::BadParameter("index"))?;
            let scale = parse_scale(query)?;
            Ok(PageRequest { id, index, scale })
        }
        _ => Err(ProtocolError::UnknownRoute(decoded.into_owned())),
    }
}

fn parse_scale(query: Option<&str>) -> Result<f32, ProtocolError> {
    let Some(query) = query else {
        return Ok(1.0);
    };
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("scale=") {
            let scale: f32 = value
                .parse()
                .map_err(|_| ProtocolError::BadParameter("scale"))?;
            if !scale.is_finite() {
                return Err(ProtocolError::BadParameter("scale"));
            }
            return Ok(scale.clamp(MIN_SCALE, MAX_SCALE));
        }
    }
    Ok(1.0)
}

/// Enregistre le protocole `mega` sur le constructeur d'application.
pub fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_uri_scheme_protocol(SCHEME, |ctx, request| handle(&ctx, &request))
}

fn handle<R: Runtime>(
    ctx: &UriSchemeContext<'_, R>,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri();
    let registry = ctx.app_handle().state::<SharedRegistry>();
    respond(&registry, uri.path(), uri.query())
}

/// Construit la réponse HTTP pour `path` + `query`, à partir du registre.
///
/// Séparée de `handle` (qui dépend du contexte Tauri) pour être testable directement :
/// 400 si l'URL est mal formée, 404 si le document ou la page n'existent pas, 200 + PNG sinon.
pub fn respond(registry: &SharedRegistry, path: &str, query: Option<&str>) -> Response<Vec<u8>> {
    let page = match parse_page_request(path, query) {
        Ok(p) => p,
        Err(e) => return text_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let guard = match lock_registry(registry) {
        Ok(g) => g,
        Err(e) => return text_response(StatusCode::INTERNAL_SERVER_ERROR, &e.message),
    };
    let Some(doc) = guard.get(page.id) else {
        return text_response(StatusCode::NOT_FOUND, "document inconnu");
    };
    let png = doc
        .render_page(page.index, page.scale)
        .and_then(|r| r.to_png());
    match png {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "no-store")
            .body(bytes)
            .expect("en-têtes valides"),
        Err(mega_core::CoreError::PageOutOfRange { .. }) => {
            text_response(StatusCode::NOT_FOUND, "page inconnue")
        }
        Err(e) => text_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn text_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .expect("en-têtes valides")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::DocumentRegistry;
    use mega_core::Document;

    fn registry_with_one_doc() -> (SharedRegistry, DocId) {
        let mut reg = DocumentRegistry::default();
        let id = reg.insert(
            Document::from_bytes(mega_core::testing::sample_pdf(1, "proto"), None).unwrap(),
        );
        (std::sync::Mutex::new(reg), id)
    }

    #[test]
    fn respond_returns_400_on_bad_route() {
        let (reg, _) = registry_with_one_doc();
        assert_eq!(
            respond(&reg, "thumb/1/0", None).status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn respond_returns_404_on_unknown_document() {
        let (reg, id) = registry_with_one_doc();
        let r = respond(&reg, &format!("page/{}/0", id + 1), None);
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn respond_returns_404_on_unknown_page() {
        let (reg, id) = registry_with_one_doc();
        let r = respond(&reg, &format!("page/{id}/7"), None);
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn respond_returns_png_for_valid_page() {
        let (reg, id) = registry_with_one_doc();
        let r = respond(&reg, &format!("page%2F{id}%2F0"), Some("scale=0.5"));
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(&r.body()[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn parse_ok() {
        let r = parse_page_request("page/3/0", Some("scale=2")).unwrap();
        assert_eq!((r.id, r.index, r.scale), (3, 0, 2.0));
    }

    #[test]
    fn parse_decodes_percent_encoding() {
        assert_eq!(parse_page_request("page%2F3%2F1", None).unwrap().index, 1);
    }

    #[test]
    fn parse_defaults_scale_to_one() {
        assert_eq!(parse_page_request("page/1/0", None).unwrap().scale, 1.0);
    }

    #[test]
    fn parse_rejects_bad_index() {
        assert!(parse_page_request("page/3/abc", None).is_err());
    }

    #[test]
    fn parse_rejects_unknown_route() {
        assert!(parse_page_request("thumb/3/0", None).is_err());
    }

    #[test]
    fn scale_is_clamped() {
        assert_eq!(
            parse_page_request("page/1/0", Some("scale=99"))
                .unwrap()
                .scale,
            8.0
        );
        assert_eq!(
            parse_page_request("page/1/0", Some("scale=0.001"))
                .unwrap()
                .scale,
            0.1
        );
    }
}
