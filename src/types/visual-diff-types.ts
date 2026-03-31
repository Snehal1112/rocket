// Types for the visual diff view — field-by-field comparison of request changes.

/** A single labeled field that may have changed between old and new versions. */
export interface FieldChange<T> {
  label: string;
  oldValue: T | undefined;
  newValue: T | undefined;
  /** True when old and new differ by JSON.stringify comparison. */
  changed: boolean;
}

/** A single row in a key-value list (headers, query params, path params). */
export interface RowChange {
  key: string;
  oldRow: { value: string; enabled: boolean } | undefined;
  newRow: { value: string; enabled: boolean } | undefined;
  status: 'added' | 'removed' | 'modified' | 'unchanged';
}

/** Structured diff of a single JSON request file. */
export interface RequestDiff {
  method: FieldChange<string>;
  url: FieldChange<string>;
  headers: RowChange[];
  queryParams: RowChange[];
  pathParams: RowChange[];
  body: FieldChange<{ mode: string; content: string | undefined }>;
  auth: FieldChange<string>;
  preRequestScript: FieldChange<string>;
  postResponseScript: FieldChange<string>;
  /** True when at least one field has changed. */
  hasChanges: boolean;
}
