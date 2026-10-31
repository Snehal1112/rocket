import { create } from 'zustand';

export type SettingsSection = 'proxy' | 'secret-managers' | 'ai' | 'support' | 'about';

interface SettingsState {
  open: boolean;
  /** The section to show. Null until one is picked; the page then shows its first section. */
  section: SettingsSection | null;
  /** True while a section must not be left, for example during an AI Agent save. */
  locked: boolean;
  openSettings: (section?: SettingsSection) => void;
  closeSettings: () => void;
  toggleSettings: () => void;
  setSection: (section: SettingsSection) => void;
  setLocked: (locked: boolean) => void;
  reset: () => void;
}

const initialState = {
  open: false,
  section: null as SettingsSection | null,
  locked: false,
};

// Global Settings page state. It is not saved, so Settings starts closed after a restart.
export const useSettingsStore = create<SettingsState>()((set, get) => ({
  ...initialState,

  openSettings(section) {
    // A locked page is already open, and it must not move to another section.
    if (get().locked) return;
    set(section ? { open: true, section } : { open: true });
  },

  closeSettings() {
    if (get().locked) return;
    set({ open: false });
  },

  toggleSettings() {
    if (get().locked) return;
    set({ open: !get().open });
  },

  setSection(section) {
    if (get().locked) return;
    set({ section });
  },

  setLocked(locked) {
    set({ locked });
  },

  reset() {
    set({ ...initialState });
  },
}));
