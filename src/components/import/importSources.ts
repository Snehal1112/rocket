export type SourceKind = 'folder' | 'zip' | 'postman-json' | 'wsdl-file' | 'openapi-file';

/** Short human label for the selected source, shown under its name. */
export function describeSource(kind: SourceKind): string {
  switch (kind) {
    case 'zip':
      return 'ZIP archive';
    case 'postman-json':
      return 'Postman Collection JSON';
    case 'wsdl-file':
      return 'WSDL file';
    case 'openapi-file':
      return 'OpenAPI file';
    default:
      return 'Folder';
  }
}
