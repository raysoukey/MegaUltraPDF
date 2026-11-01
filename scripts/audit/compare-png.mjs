// SPDX-License-Identifier: GPL-3.0-or-later
// Compare deux PNG (rendus de la même page par deux moteurs) pour l'audit de compatibilité :
// redimensionne le second à la taille du premier, convertit en gris, et imprime le pourcentage de pixels
// qui diffèrent de plus de 64 niveaux. Un écart sous 2 % est considéré comme un même rendu.
// Usage : node scripts/audit/compare-png.mjs a.png b.png   → imprime « 1.23 » (pour cent)
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const canvasMod = join(here, '..', '..', 'tools', 'pdfjs', 'node_modules', '@napi-rs', 'canvas', 'index.js');
const { createCanvas, loadImage } = await import(canvasMod.replace(/\\/g, '/').replace(/^([A-Za-z]):/, 'file:///$1:'));

const [a, b] = process.argv.slice(2);
const ia = await loadImage(readFileSync(a));
const ib = await loadImage(readFileSync(b));
const w = Math.min(ia.width, 600);
const h = Math.round((ia.height * w) / ia.width);
function gray(img) {
  const c = createCanvas(w, h);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, w, h);
  ctx.drawImage(img, 0, 0, w, h);
  const d = ctx.getImageData(0, 0, w, h).data;
  const out = new Uint8Array(w * h);
  for (let i = 0; i < w * h; i++) {
    const alpha = d[i * 4 + 3] / 255;
    const r = d[i * 4] * alpha + 255 * (1 - alpha);
    const g = d[i * 4 + 1] * alpha + 255 * (1 - alpha);
    const bl = d[i * 4 + 2] * alpha + 255 * (1 - alpha);
    out[i] = (r * 299 + g * 587 + bl * 114) / 1000;
  }
  return out;
}
const ga = gray(ia);
const gb = gray(ib);
let diff = 0;
for (let i = 0; i < ga.length; i++) if (Math.abs(ga[i] - gb[i]) > 64) diff++;
console.log(((100 * diff) / ga.length).toFixed(2));
