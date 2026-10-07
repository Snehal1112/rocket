import { type Auth, type FolderSettings, getFolderSettings, type Header } from '@/lib/tauri-api';

// Folder paths above a request, outermost first. `users/admin/get.yml` gives
// `['users', 'users/admin']`. A request at the collection root has none.
export function ancestorFolderPaths(requestPath: string): string[] {
  const segments = requestPath.split(/[\\/]/).filter((s) => s.length > 0);
  const paths: string[] = [];
  for (let i = 1; i < segments.length; i++) {
    paths.push(segments.slice(0, i).join('/'));
  }
  return paths;
}

// Loads the settings of every folder above a request, outermost first.
// A folder that cannot be read is skipped here. The backend loads the same
// chain at send time and reports a broken folder.yml as a request error.
export async function loadFolderChain(
  collection: string,
  requestPath: string,
): Promise<FolderSettings[]> {
  const chain: FolderSettings[] = [];
  for (const folderPath of ancestorFolderPaths(requestPath)) {
    try {
      chain.push(await getFolderSettings(collection, folderPath));
    } catch {
      // Skipped on purpose. The backend send reports this folder's error.
    }
  }
  return chain;
}

// Mirrors rocket_collection::inherited_headers. Collection headers come first,
// then each folder from outermost to innermost. A later level replaces an
// earlier header with the same name. Disabled headers never shadow and are
// dropped. Names compare without case, like the request merge below it; the
// Rust helper compares them exactly.
export function inheritedHeaders(collection: Header[], folders: FolderSettings[]): Header[] {
  let merged = collection.filter((h) => h.enabled);
  for (const folder of folders) {
    const own = (folder.headers ?? []).filter((h) => h.enabled);
    const keys = new Set(own.map((h) => h.key.toLowerCase()));
    merged = [...merged.filter((h) => !keys.has(h.key.toLowerCase())), ...own];
  }
  return merged;
}

// Mirrors rocket_collection::resolve_folder_auth: the innermost folder auth
// that is not `none` or `inherit`.
export function resolveFolderAuth(folders: FolderSettings[]): Auth | undefined {
  for (const folder of [...folders].reverse()) {
    const auth = folder.auth;
    if (auth && auth.authType !== 'none' && auth.authType !== 'inherit') return auth;
  }
  return undefined;
}
