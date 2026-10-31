// SPDX-License-Identifier: GPL-3.0-or-later
// Contrôle croisé par pdf.js (le moteur de Firefox) pour l'audit de compatibilité :
// ouvre un PDF, compte les pages, extrait le texte d'une page, liste les annotations et peut rendre
// une page en PNG. Imprime un objet JSON par fichier. pdfjs-dist vit dans tools/pdfjs (ignoré par Git) :
//   cd tools/pdfjs && npm install pdfjs-dist@5.4.149 @napi-rs/canvas@0.1.80
// Usage : node scripts/audit/pdfjs-check.mjs [--page N] [--password P] [--png sortie.png] fichier.pdf…
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const tools = join(here, '..', '..', 'tools', 'pdfjs', 'node_modules');
const pdfjs = await import(join(tools, 'pdfjs-dist', 'legacy', 'build', 'pdf.mjs').replace(/\\/g, '/').replace(/^([A-Za-z]):/, 'file:///$1:'));

const args = process.argv.slice(2);
let page = 1;
let password;
let png;
const files = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--page') page = Number(args[++i]);
  else if (args[i] === '--password') password = args[++i];
  else if (args[i] === '--png') png = args[++i];
  else files.push(args[i]);
}

for (const file of files) {
  const warnings = [];
  const result = { file, ok: false, pages: null, text: '', annotations: [], warnings, error: null };
  try {
    const data = new Uint8Array(readFileSync(file));
    const task = pdfjs.getDocument({
      data,
      password,
      verbosity: pdfjs.VerbosityLevel.WARNINGS,
      stopAtErrors: false,
      isEvalSupported: false,
    });
    const doc = await task.promise;
    result.pages = doc.numPages;
    const meta = await doc.getMetadata().catch((e) => ({ error: String(e) }));
    result.producer = meta?.info?.Producer ?? null;
    result.pdfFormatVersion = meta?.info?.PDFFormatVersion ?? null;
    result.isAcroForm = meta?.info?.IsAcroFormPresent ?? null;
    result.isXfa = meta?.info?.IsXFAPresent ?? null;
    result.isSigned = meta?.info?.IsSignaturesPresent ?? null;
    result.hasJavaScript = (await doc.getJSActions().catch(() => null)) !== null;
    const outline = await doc.getOutline().catch(() => null);
    result.outlineCount = outline ? outline.length : 0;
    const attachments = await doc.getAttachments().catch(() => null);
    result.attachmentCount = attachments ? Object.keys(attachments).length : 0;
    const n = Math.min(Math.max(1, page), doc.numPages);
    const p = await doc.getPage(n);
    const content = await p.getTextContent({ includeMarkedContent: false });
    result.text = content.items
      .map((it) => (it.str ?? '') + (it.hasEOL ? '\n' : ''))
      .join('')
      .replace(/\s+/g, ' ')
      .trim();
    const annots = await p.getAnnotations({ intent: 'display' });
    result.annotations = annots.map((a) => ({
      subtype: a.subtype,
      hasAppearance: Boolean(a.hasAppearance),
      fieldType: a.fieldType ?? null,
      fieldValue: a.fieldValue ?? null,
    }));
    if (png) {
      const { createCanvas } = await import(
        join(tools, '@napi-rs', 'canvas', 'index.js').replace(/\\/g, '/').replace(/^([A-Za-z]):/, 'file:///$1:')
      );
      const viewport = p.getViewport({ scale: 1.0 });
      const canvas = createCanvas(Math.ceil(viewport.width), Math.ceil(viewport.height));
      const ctx = canvas.getContext('2d');
      ctx.fillStyle = '#fff';
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      await p.render({ canvasContext: ctx, viewport, annotationMode: pdfjs.AnnotationMode.ENABLE }).promise;
      const { writeFileSync } = await import('node:fs');
      writeFileSync(png, canvas.toBuffer('image/png'));
      result.png = png;
    }
    result.ok = true;
    await doc.destroy();
  } catch (e) {
    result.error = `${e?.name ?? 'Error'}: ${e?.message ?? e}`;
  }
  console.log(JSON.stringify(result));
}
