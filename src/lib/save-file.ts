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
