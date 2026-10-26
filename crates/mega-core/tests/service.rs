// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests de l'acteur PDFium (`mega_core::service`).

use std::sync::Arc;
use std::time::Duration;

use mega_core::service::{PdfService, Priority, RenderRequest, ServiceHandle};
use mega_core::testing::sample_pdf;
use mega_core::{CoreError, Rotation, SearchOptions};

fn request(doc: u32, page: u16, generation: u64, priority: Priority) -> RenderRequest {
    RenderRequest {
        doc,
        page,
        scale: 0.5,
        rotation: Rotation::None,
        generation,
        priority,
    }
}

fn open(handle: &ServiceHandle, pages: u16) -> u32 {
    handle
        .open_bytes(sample_pdf(pages, "service"), None)
        .unwrap()
        .id
}

#[test]
fn open_and_summary() {
    let handle = PdfService::spawn();
    let summary = handle.open_bytes(sample_pdf(3, "Résumé"), None).unwrap();
    assert_eq!(summary.page_count, 3);
    assert_eq!(summary.title.as_deref(), Some("Résumé"));
    assert_eq!(summary.pages.len(), 3);
    assert_eq!(handle.summary(summary.id).unwrap().id, summary.id);
}

#[test]
fn close_then_use_is_not_found() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.close(id).unwrap();
    assert!(matches!(
        handle.summary(id),
        Err(CoreError::NotFound { id: missing }) if missing == id
    ));
    assert!(matches!(handle.close(id), Err(CoreError::NotFound { .. })));
}

#[test]
fn render_png_is_cached() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let first = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    let second = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        Arc::ptr_eq(&first, &second),
        "le second rendu doit venir du cache"
    );
}

#[test]
fn set_generation_cancels_older_requests_but_keeps_cache() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let first = handle
        .render_png(request(id, 0, 0, Priority::Visible))
        .unwrap();
    handle.set_generation(id, 3).unwrap();
    assert_eq!(handle.summary(id).unwrap().generation, 3);
    // Génération périmée : refusée sans travail.
    assert!(matches!(
        handle.render_png(request(id, 0, 2, Priority::Visible)),
        Err(CoreError::Cancelled)
    ));
    // Même échelle, génération courante : l'image vient toujours du cache (pas de rerendu).
    let again = handle
        .render_png(request(id, 0, 3, Priority::Visible))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    // Une génération plus récente que celle connue de l'acteur fait avancer le compteur
    // (le frontend est l'autorité) et n'est jamais refusée.
    assert!(
        handle
            .render_png(request(id, 0, 5, Priority::Visible))
            .is_ok()
    );
    assert_eq!(handle.summary(id).unwrap().generation, 5);
    // `set_generation` ne recule jamais.
    assert_eq!(handle.set_generation(id, 1).unwrap(), 5);
}

#[test]
fn stale_generation_is_cancelled() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.set_generation(id, 1).unwrap();
    assert!(matches!(
        handle.render_png(request(id, 0, 0, Priority::Visible)),
        Err(CoreError::Cancelled)
    ));
}

#[test]
fn render_after_close_is_not_found() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    handle.close(id).unwrap();
    assert!(matches!(
        handle.render_png(request(id, 0, 0, Priority::Visible)),
        Err(CoreError::NotFound { .. })
    ));
}

#[test]
fn render_out_of_range_page_is_reported() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    assert!(matches!(
        handle.render_png(request(id, 9, 0, Priority::Visible)),
        Err(CoreError::PageOutOfRange { .. })
    ));
}

#[test]
fn search_whole_document() {
    let handle = PdfService::spawn();
    let id = open(&handle, 3);
    let hits = handle.search(id, "Page", SearchOptions::default()).unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(
        hits.iter().map(|h| h.page).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(handle.text_boxes(id, 2).unwrap().len() > 3);
    assert!(handle.outline(id).unwrap().is_empty());
    assert!(handle.links(id, 0).unwrap().is_empty());
    assert_eq!(handle.properties(id).unwrap().page_count, 3);
}

#[test]
fn handles_are_cloneable_across_threads() {
    let handle = PdfService::spawn();
    let id = open(&handle, 2);
    let workers: Vec<_> = (0..4)
        .map(|i| {
            let h = handle.clone();
            std::thread::spawn(move || {
                let png = h
                    .render_png(request(id, (i % 2) as u16, 0, Priority::Prefetch))
                    .unwrap();
                png.len()
            })
        })
        .collect();
    for w in workers {
        assert!(w.join().unwrap() > 100);
    }
}

#[test]
fn visible_requests_served_before_thumbnails() {
    // On occupe l'acteur avec un gros rendu, on empile une vignette PUIS une page visible,
    // et on vérifie que la visible est servie d'abord.
    let handle = PdfService::spawn();
    let id = open(&handle, 3);
    let busy = {
        let h = handle.clone();
        std::thread::spawn(move || {
            h.render_png(RenderRequest {
                scale: 6.0,
                ..request(id, 0, 0, Priority::Visible)
            })
            .unwrap()
        })
    };
    std::thread::sleep(Duration::from_millis(30)); // laisse le gros rendu démarrer
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let thumb = {
        let (h, order) = (handle.clone(), Arc::clone(&order));
        std::thread::spawn(move || {
            h.render_png(request(id, 1, 0, Priority::Thumbnail))
                .unwrap();
            order.lock().unwrap().push("thumbnail");
        })
    };
    std::thread::sleep(Duration::from_millis(10));
    let visible = {
        let (h, order) = (handle.clone(), Arc::clone(&order));
        std::thread::spawn(move || {
            h.render_png(request(id, 2, 0, Priority::Visible)).unwrap();
            order.lock().unwrap().push("visible");
        })
    };
    busy.join().unwrap();
    thumb.join().unwrap();
    visible.join().unwrap();
    assert_eq!(*order.lock().unwrap(), vec!["visible", "thumbnail"]);
}

#[test]
fn dropped_handles_stop_the_service() {
    let handle = PdfService::spawn();
    let id = open(&handle, 1);
    let clone = handle.clone();
    drop(handle);
    assert!(
        clone.summary(id).is_ok(),
        "le service vit tant qu'un handle existe"
    );
}
