// SPDX-License-Identifier: GPL-3.0-or-later
// Copier du texte (Lot 13, audit P5). `navigator.clipboard.writeText` peut être refusé selon
// le WebView (WebKitGTK sous Linux, WKWebView sous macOS) ou l'absence de geste
// utilisateur reconnu. On essaie alors l'ancienne commande `copy` sur une zone de texte
// cachée, et l'interface prévient si les deux échouent, au lieu de ne rien dire.

/** Les deux façons de copier, remplaçables par les tests. */
export interface ClipboardWays {
  /** Presse-papiers asynchrone (`navigator.clipboard.writeText`). */
  write: (text: string) => Promise<void>;
  /** Ancienne commande `copy` ; vrai si elle a réussi. */
  fallback: (text: string) => boolean;
}

/** Copie `text` ; vrai si l'une des deux façons a réussi. */
export async function copyText(
  text: string,
  ways: ClipboardWays = browserWays(),
): Promise<boolean> {
  try {
    await ways.write(text);
    return true;
  } catch {
    try {
      return ways.fallback(text);
    } catch {
      return false;
    }
  }
}

/** Les façons de copier du navigateur. */
function browserWays(): ClipboardWays {
  return {
    write: (text) => navigator.clipboard.writeText(text),
    fallback: (text) => {
      const area = document.createElement('textarea');
      area.value = text;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.append(area);
      area.select();
      // `execCommand` est déprécié, mais c'est le seul repli quand l'API asynchrone manque.
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    },
  };
}
