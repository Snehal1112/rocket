import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { xml } from '@codemirror/lang-xml';
import { yaml } from '@codemirror/lang-yaml';
import type { Extension } from '@codemirror/state';

export function getLanguageExtension(lang: string): Extension | null {
  switch (lang) {
    case 'json':
      return json();
    case 'xml':
      return xml();
    case 'html':
      return html();
    case 'javascript':
      return javascript();
    case 'css':
      return css();
    case 'yaml':
      return yaml();
    default:
      return null;
  }
}

export function detectLanguage(bodyMode?: string, contentType?: string): string {
  if (bodyMode === 'json' || contentType?.includes('json')) return 'json';
  if (bodyMode === 'xml' || contentType?.includes('xml')) return 'xml';
  if (bodyMode === 'text') return 'plaintext';
  if (contentType?.includes('html')) return 'html';
  if (contentType?.includes('javascript')) return 'javascript';
  if (contentType?.includes('css')) return 'css';
  if (contentType?.includes('yaml')) return 'yaml';
  return 'plaintext';
}

export function getLanguageExtensionForFile(filePath: string): Extension | null {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, () => Extension> = {
    json: () => json(),
    js: () => javascript(),
    ts: () => javascript({ typescript: true }),
    tsx: () => javascript({ typescript: true, jsx: true }),
    jsx: () => javascript({ jsx: true }),
    yaml: () => yaml(),
    yml: () => yaml(),
    xml: () => xml(),
    html: () => html(),
    css: () => css(),
  };
  const factory = map[ext];
  return factory ? factory() : null;
}
