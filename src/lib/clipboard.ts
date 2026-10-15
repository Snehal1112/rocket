import { isTauri } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// Copies text that is computed asynchronously. WebKit drops the click's user
// activation after an await, so the web clipboard write must start synchronously
// and receive the text as a promise. The native plugin is the fallback, because
// it can fail on Linux when no X11 server is reachable (a Wayland session).
export async function copyTextAsync(textPromise: Promise<string>): Promise<void> {
  // Start the web write now, while the click's user activation is still valid.
  if (await tryWebItemWrite(textPromise)) return;

  const text = await textPromise;
  if (isTauri()) {
    try {
      await writeText(text);
      return;
    } catch (err) {
      console.warn('Native clipboard write failed, trying the web API.', err);
    }
  }
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    legacyCopy(text);
  }
}

// Writes a promised blob to the clipboard. Returns false when the API is missing or rejects.
async function tryWebItemWrite(textPromise: Promise<string>): Promise<boolean> {
  if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) return false;
  try {
    const blob = textPromise.then((text) => new Blob([text], { type: 'text/plain' }));
    await navigator.clipboard.write([new ClipboardItem({ 'text/plain': blob })]);
    return true;
  } catch (err) {
    console.warn('Async clipboard write failed, trying the fallback.', err);
    return false;
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
