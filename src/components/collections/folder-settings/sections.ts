import type { FolderSettings } from '@/lib/tauri-api';
import type { FolderSection } from '@/types/pane-types';

/** The six folder sections in display order. */
export const FOLDER_SECTIONS: ReadonlyArray<{ id: FolderSection; label: string }> = [
  { id: 'headers', label: 'Headers' },
  { id: 'script', label: 'Script' },
  { id: 'test', label: 'Test' },
  { id: 'vars', label: 'Vars' },
  { id: 'auth', label: 'Auth' },
  { id: 'docs', label: 'Docs' },
];

export function folderSectionLabel(section: FolderSection): string {
  return FOLDER_SECTIONS.find((s) => s.id === section)?.label ?? section;
}

export function isFolderSection(value: string): value is FolderSection {
  return FOLDER_SECTIONS.some((s) => s.id === value);
}

/** Props every folder section body receives. */
export interface FolderSectionProps {
  collectionName: string;
  /** Folder path relative to the collection root. */
  folderPath: string;
  /** Supplied by the tab once plan 09 adds the settings hook. Placeholders ignore it. */
  settings?: FolderSettings;
  onChange?: (patch: Partial<FolderSettings>) => void;
}
