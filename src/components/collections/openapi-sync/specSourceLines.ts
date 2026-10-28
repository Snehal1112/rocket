/** Splits spec text into lines once. A trailing carriage return of CRLF text is dropped. */
export function splitLines(text: string): string[] {
  return text.split('\n').map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l));
}

function indentOf(line: string): number {
  let i = 0;
  while (i < line.length && (line[i] === ' ' || line[i] === '\t')) i++;
  return i;
}

function isBlankOrComment(line: string): boolean {
  const t = line.trim();
  return t === '' || t.startsWith('#');
}

/** The key of a YAML mapping line (`key:`), unquoted, or null when the line is not a key. */
function yamlKey(trimmed: string): string | null {
  const q = trimmed[0];
  if (q === '"' || q === "'") {
    const end = trimmed.indexOf(q, 1);
    if (end < 0) return null;
    const rest = trimmed.slice(end + 1).trimStart();
    return rest.startsWith(':') ? trimmed.slice(1, end) : null;
  }
  // A bare key ends at the first colon followed by a space or the end of the line.
  const m = trimmed.match(/^(.*?):(?:[ \t]|$)/);
  if (!m || m[1] === '') return null;
  return m[1].trimEnd();
}

function findYaml(lines: string[], path: string, method: string): number | null {
  let i = 0;
  while (i < lines.length && !/^paths\s*:/.test(lines[i])) i++;
  if (i >= lines.length) return null;
  let pathIndent = -1;
  let inPath = false;
  let methodIndent = -1;
  for (i += 1; i < lines.length; i++) {
    const line = lines[i];
    if (isBlankOrComment(line)) continue;
    const indent = indentOf(line);
    // A line back at column 0 ends the paths block.
    if (indent === 0) return null;
    if (pathIndent < 0) pathIndent = indent;
    const key = yamlKey(line.trim());
    if (indent === pathIndent) {
      if (inPath) return null;
      inPath = key === path;
      continue;
    }
    if (!inPath || indent < pathIndent) continue;
    if (methodIndent < 0) methodIndent = indent;
    if (indent === methodIndent && key !== null && key.toLowerCase() === method) return i + 1;
  }
  return null;
}

function findJson(lines: string[], path: string, method: string): number | null {
  const pathsKey = '"paths"';
  const pathKey = JSON.stringify(path);
  const methodKey = `"${method}"`;
  let i = 0;
  while (i < lines.length && !lines[i].includes(pathsKey)) i++;
  if (i >= lines.length) return null;
  const pathsIndent = indentOf(lines[i]);
  let inPath = false;
  let pathIndent = -1;
  let methodIndent = -1;
  for (i += 1; i < lines.length; i++) {
    const line = lines[i];
    const t = line.trim();
    if (t === '') continue;
    const indent = indentOf(line);
    // A closing brace at or above the paths key ends the block.
    if (indent <= pathsIndent && (t.startsWith('}') || t.startsWith(']'))) return null;
    if (pathIndent < 0) pathIndent = indent;
    if (indent === pathIndent && t.startsWith('"')) {
      inPath = t.startsWith(`${pathKey}:`) || t.startsWith(`${pathKey} :`);
      methodIndent = -1;
      continue;
    }
    // The closing brace of the path object ends the path.
    if (indent <= pathIndent && t.startsWith('}')) inPath = false;
    if (!inPath || indent <= pathIndent) continue;
    // The first key line under the path sets the indent of the methods.
    if (methodIndent < 0) methodIndent = indent;
    if (indent === methodIndent && t.toLowerCase().startsWith(`${methodKey}:`)) return i + 1;
  }
  return null;
}

/** The 1-based line of the method key under a path in the spec source, or null. */
export function findOperationLine(
  lines: string[],
  format: 'yaml' | 'json',
  path: string,
  method: string,
): number | null {
  const m = method.toLowerCase();
  return format === 'yaml' ? findYaml(lines, path, m) : findJson(lines, path, m);
}
