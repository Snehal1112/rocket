import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { type FolderSettings, getFolderSettings, saveFolderSettings } from '@/lib/tauri-api';

const EMPTY_FOLDER_SETTINGS: FolderSettings = { headers: [], variables: [] };

export interface UseFolderSettingsResult {
  settings: FolderSettings;
  setSettings: (next: FolderSettings | ((prev: FolderSettings) => FolderSettings)) => void;
  isDirty: boolean;
  isLoaded: boolean;
  /** Set when loading the folder settings failed. */
  error: string | null;
  /** Writes the whole settings object. Does nothing when there is nothing to save. */
  save: () => Promise<void>;
  saveState: SaveButtonState;
}

/** Copies onto `current` only the fields that differ between `baseline` and `edited`. */
function applyEdits(
  current: FolderSettings,
  baseline: FolderSettings,
  edited: FolderSettings,
): FolderSettings {
  const result: Record<string, unknown> = { ...current };
  const before = baseline as unknown as Record<string, unknown>;
  const after = edited as unknown as Record<string, unknown>;
  for (const key of new Set([...Object.keys(before), ...Object.keys(after)])) {
    if (JSON.stringify(before[key]) === JSON.stringify(after[key])) continue;
    if (after[key] === undefined) delete result[key];
    else result[key] = after[key];
  }
  return result as unknown as FolderSettings;
}

/**
 * Loads one folder's settings and tracks edits, dirty state and saving.
 * Refs hold the latest values so a late response or a late save never
 * touches the folder the tab shows now.
 */
export function useFolderSettings(collection: string, folderPath: string): UseFolderSettingsResult {
  const [settings, setSettingsState] = useState<FolderSettings>(EMPTY_FOLDER_SETTINGS);
  const [isDirty, setIsDirty] = useState(false);
  const [isLoaded, setIsLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveButtonState>('idle');

  const settingsRef = useRef<FolderSettings>(EMPTY_FOLDER_SETTINGS);
  // The last settings known to match the file. Edits are diffed against it on save.
  const baselineRef = useRef<FolderSettings>(EMPTY_FOLDER_SETTINGS);
  const loadedRef = useRef(false);
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const mountedRef = useRef(true);
  const editVersionRef = useRef(0);
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // The key names the folder this hook currently shows.
  const key = JSON.stringify([collection, folderPath]);
  const keyRef = useRef(key);
  keyRef.current = key;
  const collectionRef = useRef(collection);
  collectionRef.current = collection;
  const folderPathRef = useRef(folderPath);
  folderPathRef.current = folderPath;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (successTimerRef.current) clearTimeout(successTimerRef.current);
    };
  }, []);

  // Load on mount and whenever the folder changes. The flag drops stale responses.
  useEffect(() => {
    let active = true;
    settingsRef.current = EMPTY_FOLDER_SETTINGS;
    baselineRef.current = EMPTY_FOLDER_SETTINGS;
    loadedRef.current = false;
    dirtyRef.current = false;
    editVersionRef.current += 1;
    if (successTimerRef.current) clearTimeout(successTimerRef.current);
    setSettingsState(EMPTY_FOLDER_SETTINGS);
    setIsDirty(false);
    setIsLoaded(false);
    setError(null);
    setSaveState('idle');

    getFolderSettings(collection, folderPath)
      .then((loaded) => {
        if (!active) return;
        settingsRef.current = loaded;
        baselineRef.current = loaded;
        loadedRef.current = true;
        setSettingsState(loaded);
        setIsLoaded(true);
      })
      .catch((err) => {
        if (!active) return;
        console.error('[useFolderSettings] load failed', err);
        setError('Failed to load folder settings.');
      });

    return () => {
      active = false;
    };
  }, [collection, folderPath]);

  const setSettings = useCallback(
    (next: FolderSettings | ((prev: FolderSettings) => FolderSettings)) => {
      // Never edit before a successful load, so an empty object cannot be saved over the file.
      if (!loadedRef.current) return;
      const value = typeof next === 'function' ? next(settingsRef.current) : next;
      settingsRef.current = value;
      editVersionRef.current += 1;
      dirtyRef.current = true;
      setSettingsState(value);
      setIsDirty(true);
    },
    [],
  );

  const save = useCallback(async () => {
    if (savingRef.current || !loadedRef.current || !dirtyRef.current) return;
    const savedKey = keyRef.current;
    const savedCollection = collectionRef.current;
    const savedPath = folderPathRef.current;
    const snapshot = settingsRef.current;
    const baseline = baselineRef.current;
    const savedVersion = editVersionRef.current;

    savingRef.current = true;
    if (successTimerRef.current) clearTimeout(successTimerRef.current);
    setSaveState('saving');
    try {
      // Read the current file first, so fields this tab did not edit are never overwritten.
      const current = await getFolderSettings(savedCollection, savedPath);
      await saveFolderSettings(savedCollection, savedPath, applyEdits(current, baseline, snapshot));
      if (!mountedRef.current || keyRef.current !== savedKey) return;
      baselineRef.current = snapshot;
      if (editVersionRef.current === savedVersion) {
        dirtyRef.current = false;
        setIsDirty(false);
      }
      setSaveState('success');
      successTimerRef.current = setTimeout(() => {
        if (mountedRef.current) setSaveState('idle');
      }, 2000);
    } catch (err) {
      console.error('[useFolderSettings] save failed', err);
      toast.error('Failed to save folder settings');
      if (mountedRef.current && keyRef.current === savedKey) setSaveState('idle');
    } finally {
      savingRef.current = false;
      // A save that ended for another folder must not leave this one stuck in saving.
      if (mountedRef.current && keyRef.current !== savedKey) setSaveState('idle');
    }
  }, []);

  return { settings, setSettings, isDirty, isLoaded, error, save, saveState };
}
