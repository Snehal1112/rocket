import type { KeyValueEntry } from '@/types/pane-types';

type HeaderLike = Pick<KeyValueEntry, 'key' | 'value'>;

const PREVIEWABLE_IMAGES = new Set([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'image/bmp',
  'image/avif',
  'image/svg+xml',
  'image/x-icon',
]);

const EXTENSIONS: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'image/svg+xml': 'svg',
  'application/pdf': 'pdf',
  'application/zip': 'zip',
  'application/gzip': 'gz',
  'audio/mpeg': 'mp3',
  'video/mp4': 'mp4',
};

export function base64ToBytes(b64: string): Uint8Array {
  const raw = atob(b64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

export function isPreviewableImage(contentType: string): boolean {
  return PREVIEWABLE_IMAGES.has(contentType.split(';')[0].trim().toLowerCase());
}

// The file name offered by the save dialog: the server's name when it sends one, else
// "response" plus an extension for the content type. Any directory part is dropped.
export function suggestedFileName(headers: HeaderLike[], contentType: string): string {
  const disposition = headers.find((h) => h.key.toLowerCase() === 'content-disposition')?.value;
  const match = disposition ? /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(disposition) : null;
  if (match) {
    let name = match[1];
    try {
      name = decodeURIComponent(name);
    } catch {
      // Keep the raw value when it is not valid percent-encoding.
    }
    const base = name.split(/[\\/]/).pop()?.trim();
    if (base) return base;
  }
  const essence = contentType.split(';')[0].trim().toLowerCase();
  return `response.${EXTENSIONS[essence] ?? 'bin'}`;
}
