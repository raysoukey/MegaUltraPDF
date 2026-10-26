// SPDX-License-Identifier: GPL-3.0-or-later
//! Le protocole `mega://` : sert les pages rendues en PNG au frontend.
//!
//! Le frontend affiche `<img src="…/page/<id>/<index>?scale=2&rot=90&gen=3&prio=visible">`.
//! Plutôt que de faire transiter l'image par `invoke` (JSON, base64), on enregistre un
//! protocole personnalisé : le WebView adresse la requête au Rust, qui répond avec les
//! octets PNG, comme un serveur HTTP local mais sans réseau. Sous Windows, le WebView voit
//! ce protocole sous la forme `http://mega.localhost/…`.
//!
//! Le protocole est **asynchrone** : le handler rend la main immédiatement et la réponse
//! est produite sur le pool `spawn_blocking`, où l'attente de l'acteur PDFium ne gêne
//! personne. `convertFileSrc` (côté TS) encode le chemin en pourcentage (`page%2F3%2F0`) ;
//! on le décode donc avant de l'analyser.

use mega_core::service::{DocId, Priority, RenderRequest, ServiceHandle};
use mega_core::{CoreError, Rotation};
use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

/// Nom du protocole, tel qu'enregistré auprès de Tauri et utilisé dans la CSP.
pub const SCHEME: &str = "mega";

const MIN_SCALE: f32 = 0.1;
const MAX_SCALE: f32 = 8.0;

/// Une demande de page analysée depuis l'URL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRequest {
    /// Identifiant du document dans l'acteur.
    pub id: DocId,
    /// Index de page, à partir de 0.
    pub index: u16,
    /// Échelle de rendu, déjà bornée à [`MIN_SCALE`, `MAX_SCALE`].
    pub scale: f32,
    /// Rotation d'affichage (`rot=0|90|180|270`, défaut 0).
    pub rotation: Rotation,
    /// Génération attendue (`gen=`, défaut 0).
    pub generation: u64,
    /// Priorité (`prio=visible|prefetch|thumbnail`, défaut `visible`).
    pub priority: Priority,
}

impl PageRequest {
    fn to_render_request(self) -> RenderRequest {
        RenderRequest {
            doc: self.id,
            page: self.index,
            scale: self.scale,
            rotation: self.rotation,
            generation: self.generation,
            priority: self.priority,
        }
    }
}

/// Pourquoi une URL n'est pas une demande de page valide.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ProtocolError {
    /// Le chemin ne commence pas par `page/<id>/<index>`.
    #[error("route inconnue : {0}")]
    UnknownRoute(String),
    /// Un paramètre n'est pas valide.
    #[error("paramètre invalide : {0}")]
    BadParameter(&'static str),
}

/// Analyse `page/<id>/<index>` (éventuellement percent-encodé) et la requête
/// `scale=&rot=&gen=&prio=`.
///
/// L'échelle est bornée à [0,1 ; 8] pour qu'une URL forgée ne demande pas une image
/// de plusieurs gigaoctets.
pub fn parse_page_request(path: &str, query: Option<&str>) -> Result<PageRequest, ProtocolError> {
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    let mut parts = decoded.trim_start_matches('/').split('/');
    let (id, index) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("page"), Some(id), Some(index), None) => (
            id.parse::<DocId>()
                .map_err(|_| ProtocolError::BadParameter("id"))?,
            index
                .parse::<u16>()
                .map_err(|_| ProtocolError::BadParameter("index"))?,
        ),
        _ => return Err(ProtocolError::UnknownRoute(decoded.into_owned())),
    };
    let mut req = PageRequest {
        id,
        index,
        scale: 1.0,
        rotation: Rotation::None,
        generation: 0,
        priority: Priority::Visible,
    };
    for (key, value) in query
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
    {
        match key {
            "scale" => {
                let scale: f32 = value
                    .parse()
                    .map_err(|_| ProtocolError::BadParameter("scale"))?;
                if !scale.is_finite() {
                    return Err(ProtocolError::BadParameter("scale"));
                }
                req.scale = scale.clamp(MIN_SCALE, MAX_SCALE);
            }
            "rot" => {
                let deg: u16 = value
                    .parse()
                    .map_err(|_| ProtocolError::BadParameter("rot"))?;
                req.rotation = Rotation::from_degrees(deg);
            }
            "gen" => {
                req.generation = value
                    .parse()
                    .map_err(|_| ProtocolError::BadParameter("gen"))?;
            }
            "prio" => {
                req.priority = match value {
                    "visible" => Priority::Visible,
                    "prefetch" => Priority::Prefetch,
                    "thumbnail" => Priority::Thumbnail,
                    _ => return Err(ProtocolError::BadParameter("prio")),
                };
            }
            _ => {} // paramètre inconnu : ignoré
        }
    }
    Ok(req)
}

/// Enregistre le protocole `mega` (asynchrone) sur le constructeur d'application.
pub fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(SCHEME, handle)
}

fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let service = ctx.app_handle().state::<ServiceHandle>().inner().clone();
    let uri = request.uri().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let response = respond(&service, uri.path(), uri.query());
        responder.respond(response);
    });
}

/// Construit la réponse HTTP pour `path` + `query` via l'acteur.
///
/// Séparée de `handle` (qui dépend du contexte Tauri) pour être testable directement :
/// 400 si l'URL est mal formée, 404 si document ou page inconnus, 409 si la génération est
/// périmée, 200 + PNG sinon.
// `Response::builder()` n'échoue que si un en-tête est invalide ; les nôtres sont des
// constantes vérifiées par les tests, d'où l'`expect` toléré (ADR-0005).
#[allow(clippy::expect_used)]
pub fn respond(service: &ServiceHandle, path: &str, query: Option<&str>) -> Response<Vec<u8>> {
    let page = match parse_page_request(path, query) {
        Ok(p) => p,
        Err(e) => return text_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    match service.render_png(page.to_render_request()) {
        Ok(png) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "no-store")
            .body(png.as_ref().clone())
            .expect("en-têtes valides"),
        Err(CoreError::NotFound { .. }) => text_response(StatusCode::NOT_FOUND, "document inconnu"),
        Err(CoreError::PageOutOfRange { .. }) => {
            text_response(StatusCode::NOT_FOUND, "page inconnue")
        }
        Err(CoreError::Cancelled) => text_response(StatusCode::CONFLICT, "génération périmée"),
        Err(e) => text_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// Même justification que pour `respond` : en-têtes constants (ADR-0005).
#[allow(clippy::expect_used)]
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
    use mega_core::service::PdfService;

    fn service_with_one_doc() -> (ServiceHandle, DocId) {
        let service = PdfService::spawn();
        let id = service
            .open_bytes(mega_core::testing::sample_pdf(1, "proto"), None)
            .unwrap()
            .id;
        (service, id)
    }

    #[test]
    fn parse_ok_with_all_parameters() {
        let r =
            parse_page_request("page/3/0", Some("scale=2&rot=90&gen=7&prio=thumbnail")).unwrap();
        assert_eq!((r.id, r.index, r.scale), (3, 0, 2.0));
        assert_eq!(r.rotation, Rotation::Cw90);
        assert_eq!(r.generation, 7);
        assert_eq!(r.priority, Priority::Thumbnail);
    }

    #[test]
    fn parse_defaults() {
        let r = parse_page_request("page%2F3%2F1", None).unwrap();
        assert_eq!(r.index, 1);
        assert_eq!(r.scale, 1.0);
        assert_eq!(r.rotation, Rotation::None);
        assert_eq!(r.generation, 0);
        assert_eq!(r.priority, Priority::Visible);
    }

    #[test]
    fn parse_rejects_bad_values() {
        assert!(parse_page_request("page/3/abc", None).is_err());
        assert!(parse_page_request("thumb/3/0", None).is_err());
        assert!(parse_page_request("page/1/0", Some("prio=urgent")).is_err());
        assert!(parse_page_request("page/1/0", Some("gen=x")).is_err());
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

    #[test]
    fn respond_returns_400_on_bad_route() {
        let (s, _) = service_with_one_doc();
        assert_eq!(
            respond(&s, "thumb/1/0", None).status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn respond_returns_404_on_unknown_document_or_page() {
        let (s, id) = service_with_one_doc();
        assert_eq!(
            respond(&s, &format!("page/{}/0", id + 1), None).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(&s, &format!("page/{id}/7"), None).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn respond_cancelled_is_409() {
        let (s, id) = service_with_one_doc();
        s.set_generation(id, 1).unwrap();
        assert_eq!(
            respond(&s, &format!("page/{id}/0"), Some("gen=0")).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            respond(&s, &format!("page/{id}/0"), Some("gen=1")).status(),
            StatusCode::OK
        );
    }

    #[test]
    fn respond_returns_png_for_valid_page() {
        let (s, id) = service_with_one_doc();
        let r = respond(&s, &format!("page%2F{id}%2F0"), Some("scale=0.5&rot=90"));
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(&r.body()[..4], &[0x89, b'P', b'N', b'G']);
    }
}
