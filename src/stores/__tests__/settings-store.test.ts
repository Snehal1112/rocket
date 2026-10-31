import { beforeEach, describe, expect, it } from 'vitest';
import { useSettingsStore } from '../settings-store';

const state = () => useSettingsStore.getState();

describe('settings store', () => {
  beforeEach(() => {
    state().reset();
  });

  it('starts closed, unlocked and with no section', () => {
    expect(state().open).toBe(false);
    expect(state().locked).toBe(false);
    expect(state().section).toBeNull();
  });

  it('opens at a section and keeps the section when opened again without one', () => {
    state().openSettings('about');
    expect(state().open).toBe(true);
    expect(state().section).toBe('about');
    state().closeSettings();
    state().openSettings();
    expect(state().section).toBe('about');
  });

  it('toggles open and closed', () => {
    state().toggleSettings();
    expect(state().open).toBe(true);
    state().toggleSettings();
    expect(state().open).toBe(false);
  });

  it('changes the section', () => {
    state().openSettings();
    state().setSection('support');
    expect(state().section).toBe('support');
  });

  it('ignores close, toggle, section changes and reopening at another section while locked', () => {
    state().openSettings('ai');
    state().setLocked(true);
    state().closeSettings();
    state().toggleSettings();
    state().setSection('about');
    state().openSettings('proxy');
    expect(state().open).toBe(true);
    expect(state().section).toBe('ai');
    state().setLocked(false);
    state().closeSettings();
    expect(state().open).toBe(false);
  });

  it('reset clears open, section and lock', () => {
    state().openSettings('about');
    state().setLocked(true);
    state().reset();
    expect(state()).toMatchObject({ open: false, section: null, locked: false });
  });
});
