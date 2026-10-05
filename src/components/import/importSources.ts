export type SourceKind = 'folder' | 'zip' | 'postman-json' | 'wsdl-file';

/** Short human label for the selected source, shown under its name. */
export function describeSource(kind: SourceKind): string {
  switch (kind) {
    case 'zip':
      return 'ZIP archive';
    case 'postman-json':
      return 'Postman Collection JSON';
    case 'wsdl-file':
      return 'WSDL file';
    default:
      return 'Folder';
  }
}
