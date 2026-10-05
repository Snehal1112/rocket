export const OUTSIDE_WORKSPACE_MESSAGE = 'Files must be inside the workspace folder';

// Turns an absolute path from the file picker into a forward-slash path relative to the
// workspace folder. Returns null when the file is not inside the workspace.
export function toWorkspaceRelativePath(picked: string, workspaceRoot: string): string | null {
  const file = picked.replace(/\\/g, '/');
  const root = workspaceRoot.replace(/\\/g, '/').replace(/\/+$/, '');
  if (!root) return null;
  // Windows drive letters are case-insensitive.
  const hasDrive = /^[a-z]:/i.test(root);
  const head = file.slice(0, root.length + 1);
  const wanted = `${root}/`;
  const matches = hasDrive ? head.toLowerCase() === wanted.toLowerCase() : head === wanted;
  if (!matches) return null;
  const relative = file.slice(root.length + 1);
  if (!relative || relative.split('/').includes('..')) return null;
  return relative;
}

// Turns an absolute path from the file picker into the path stored on an upload. A file inside
// the collection folder is stored relative to it, so renaming or moving the collection keeps
// working. A file elsewhere in the workspace, or any file of a request that is not saved in a
// collection, keeps its absolute path. Returns null when the file is not inside the workspace.
export function toUploadFilePath(
  picked: string,
  workspaceRoot: string,
  collection?: string,
): string | null {
  if (toWorkspaceRelativePath(picked, workspaceRoot) === null) return null;
  if (!collection) return picked;
  const root = workspaceRoot.replace(/[\\/]+$/, '');
  return toWorkspaceRelativePath(picked, `${root}/collections/${collection}`) ?? picked;
}
