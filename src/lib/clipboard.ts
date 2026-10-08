import { isTauri } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// Copies text that is computed asynchronously. WebKit drops the click's user
// activation after an await, so the clipboard write must start synchronously
// and receive the text as a promise.
export async function copyTextAsync(textPromise: Promise<string>): Promise<void> {
  // The native plugin is not bound by webview clipboard rules, so try it first.
  if (isTauri()) {
    const text = await textPromise;
    try {
      await writeText(text);
      return;
    } catch (err) {
      console.warn('Native clipboard write failed, trying the web API.', err);
      return copyWithWebApi(Promise.resolve(text));
    }
  }
  return copyWithWebApi(textPromise);
}

async function copyWithWebApi(textPromise: Promise<string>): Promise<void> {
  if (typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
    try {
      const blob = textPromise.then((text) => new Blob([text], { type: 'text/plain' }));
      await navigator.clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
      return;
    } catch (err) {
      console.warn('Async clipboard write failed, trying the fallback.', err);
    }
  }
  const text = await textPromise;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    legacyCopy(text);
  }
}

// Last resort for webviews that reject the async clipboard API.
function legacyCopy(text: string): void {
  const area = document.createElement('textarea');
  area.value = text;
  area.style.position = 'fixed';
  area.style.opacity = '0';
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand('copy');
  document.body.removeChild(area);
  if (!ok) throw new Error('Clipboard copy is not available.');
}
