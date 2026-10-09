/** What a `#` reference points at. */
export type ReferenceKind = 'request' | 'folder' | 'collection' | 'environment' | 'last-response';

/**
 * One item of the `#` list, and the chip it becomes. `path` is the request or folder
 * path inside the collection, or the environment name when `kind` is `environment`.
 */
export interface ReferenceItem {
  kind: ReferenceKind;
  collection: string;
  path?: string;
  label: string;
}

/** One Rocket prompt template in the `/` list. */
export interface SlashCommandItem {
  name: string;
  description: string;
  template: string;
}
