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
