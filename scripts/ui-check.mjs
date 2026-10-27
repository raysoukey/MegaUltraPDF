// SPDX-License-Identifier: GPL-3.0-or-later
// Pilote la fenêtre Tauri via le protocole Chrome DevTools (WebView2 lancé avec
// `--remote-debugging-port=9222`) et déroule un scénario JSON : captures d'écran,
// évaluations JavaScript, touches, souris. Sert à vérifier l'interface sans cliquer.
//
// Usage : node scripts/ui-check.mjs <scenario.json> <dossier-de-sortie>
// Étapes possibles dans le scénario (tableau d'objets) :
//   { "wait": 800 }                       pause en millisecondes
//   { "eval": "expression", "label": "…" } évalue dans la page (promesses attendues), journalise
//   { "assert": "expression", "label": "…" } comme eval, mais échoue si le résultat est faux
//   { "shot": "nom.png" }                  capture d'écran
//   { "click": "sélecteur CSS" }           clic DOM
//   { "key": "F1", "ctrl": true, "shift": false } touche clavier (événements CDP)
//   { "type": "texte" }                    saisie de texte (Input.insertText)
//   { "drag": "sélecteur", "from": [x, y], "to": [x, y] } glisser souris (coordonnées relatives à l'élément)

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [scenarioPath, outDir = '.'] = process.argv.slice(2);
if (!scenarioPath) {
  console.error('usage: node scripts/ui-check.mjs <scenario.json> <dossier-de-sortie>');
  process.exit(2);
}
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
mkdirSync(outDir, { recursive: true });
const port = 9222;

async function findPage() {
  for (let i = 0; i < 90; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((t) => t.type === 'page' && !t.url.startsWith('devtools'));
      if (page) return page;
    } catch {
      // le WebView n'écoute pas encore
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error('aucune page CDP trouvée sur le port 9222');
}

const page = await findPage();
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
let nextId = 0;
const pending = new Map();
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = ++nextId;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });

async function evaluate(expression) {
  const res = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (res.result?.exceptionDetails) {
    throw new Error(res.result.exceptionDetails.exception?.description ?? 'exception JS');
  }
  return res.result?.result?.value;
}

async function rectOf(selector) {
  return evaluate(
    `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })()`,
  );
}

function modifiers(step) {
  return (step.alt ? 1 : 0) | (step.ctrl ? 2 : 0) | (step.shift ? 8 : 0);
}

const VIRTUAL_KEYS = { Enter: 13, Escape: 27, Home: 36, End: 35, PageUp: 33, PageDown: 34, F1: 112, F3: 114 };

async function pressKey(step) {
  const key = step.key;
  // Entrée doit porter un texte ("\r") pour déclencher la soumission implicite des formulaires.
  const text = key.length === 1 ? key : key === 'Enter' ? '\r' : undefined;
  const base = {
    key,
    code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
    windowsVirtualKeyCode: VIRTUAL_KEYS[key] ?? (key.length === 1 ? key.toUpperCase().charCodeAt(0) : undefined),
    modifiers: modifiers(step),
    text,
    unmodifiedText: text,
  };
  await send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', ...base });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
}

async function drag(step) {
  const r = await rectOf(step.drag);
  if (!r) throw new Error(`élément introuvable : ${step.drag}`);
  const [fx, fy] = step.from;
  const [tx, ty] = step.to;
  const mouse = (type, x, y, extra = {}) =>
    send('Input.dispatchMouseEvent', { type, x: r.x + x, y: r.y + y, button: 'left', ...extra });
  await mouse('mouseMoved', fx, fy);
  await mouse('mousePressed', fx, fy, { clickCount: 1 });
  const steps = 6;
  for (let i = 1; i <= steps; i++) {
    await mouse('mouseMoved', fx + ((tx - fx) * i) / steps, fy + ((ty - fy) * i) / steps, { buttons: 1 });
  }
  await mouse('mouseReleased', tx, ty, { clickCount: 1 });
}

let failures = 0;
for (const step of scenario) {
  if (step.wait !== undefined) {
    await new Promise((r) => setTimeout(r, step.wait));
  } else if (step.eval !== undefined || step.assert !== undefined) {
    const expr = step.eval ?? step.assert;
    let value;
    try {
      value = await evaluate(expr);
    } catch (e) {
      value = `ERREUR: ${e.message}`;
    }
    const ok = step.assert === undefined ? true : Boolean(value) && !String(value).startsWith('ERREUR');
    if (!ok) failures++;
    console.log(`${ok ? '✔' : '✘'} ${step.label ?? expr} → ${JSON.stringify(value)}`);
  } else if (step.media !== undefined) {
    // Émule un média CSS (« print » pour vérifier la feuille d'impression, « » pour revenir).
    await send('Emulation.setEmulatedMedia', { media: step.media });
    console.log(`🖨 média : ${step.media || '(écran)'}`);
  } else if (step.shot) {
    const cap = await send('Page.captureScreenshot', { format: 'png' });
    const file = join(outDir, step.shot);
    writeFileSync(file, Buffer.from(cap.result.data, 'base64'));
    console.log(`📷 ${file}`);
  } else if (step.click) {
    await evaluate(`(() => { const el = document.querySelector(${JSON.stringify(step.click)}); if (!el) throw new Error('introuvable'); el.click(); return true; })()`);
    console.log(`🖱 ${step.click}`);
  } else if (step.key) {
    await pressKey(step);
    console.log(`⌨ ${step.ctrl ? 'Ctrl+' : ''}${step.shift ? 'Maj+' : ''}${step.key}`);
  } else if (step.type !== undefined) {
    await send('Input.insertText', { text: step.type });
    console.log(`⌨ "${step.type}"`);
  } else if (step.drag) {
    await drag(step);
    console.log(`🖱 glisser ${step.drag} ${step.from} → ${step.to}`);
  }
}
ws.close();
console.log(failures === 0 ? 'SCÉNARIO OK' : `SCÉNARIO : ${failures} assertion(s) en échec`);
process.exit(failures === 0 ? 0 : 1);
