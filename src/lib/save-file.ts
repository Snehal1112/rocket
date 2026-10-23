import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';

export interface FileFilter {
  name: string;
  extensions: string[];
}

/**
 * Asks where to save and writes `text` there as UTF-8. Returns false when the
 * dialog was cancelled, and rejects when the write fails. Uses `writeFile` with
 * bytes, because the app's capabilities allow `fs:allow-write-file` and not the
 * text variant.
 */
export async function saveTextFile(
  defaultName: string,
  text: string,
  filters: FileFilter[],
): Promise<boolean> {
  const path = await save({ defaultPath: defaultName, filters });
  if (!path) return false;
  await writeFile(path, new TextEncoder().encode(text));
  return true;
}

/** Asks where to save and writes `bytes` there. Returns false when cancelled. */
export async function saveBinaryFile(
  defaultName: string,
  bytes: Uint8Array,
  filters: FileFilter[],
): Promise<boolean> {
  const path = await save({ defaultPath: defaultName, filters });
  if (!path) return false;
  await writeFile(path, bytes);
  return true;
}

/** Decodes a standard base64 string into bytes. */
export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
