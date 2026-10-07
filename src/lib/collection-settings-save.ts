import type { CollectionSettings } from '@/lib/tauri-api';

type EditedSettings = Pick<CollectionSettings, 'auth' | 'headers' | 'docs' | 'variables'>;

/**
 * Builds the payload for `save_collection_settings`, which replaces all settings.
 * Fields the overview tab does not edit are carried over from `current`, so a save
 * never wipes them.
 */
export function buildSettingsForSave(
  current: CollectionSettings,
  edited: EditedSettings,
): CollectionSettings {
  return {
    ...edited,
    sandboxMode: current.sandboxMode,
    scriptContextRoots: current.scriptContextRoots,
    scriptFlow: current.scriptFlow,
  };
}
