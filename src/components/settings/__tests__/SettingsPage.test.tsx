import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from '@/stores/settings-store';
import { SettingsPage } from '../SettingsPage';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('1.2.3') }));
vi.mock('@tauri-apps/plugin-os', () => ({
  platform: () => 'linux',
  version: () => '6.1',
  arch: () => 'x86_64',
}));
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getAppDataDir: vi.fn().mockResolvedValue('/data'),
  getProxySettings: vi.fn().mockResolvedValue({ mode: 'system', hasPassword: false }),
  saveProxySettings: vi.fn(),
}));

const nav = () => screen.getByRole('navigation', { name: 'Settings sections' });

describe('SettingsPage', () => {
  beforeEach(() => {
    useSettingsStore.getState().reset();
    useSettingsStore.getState().openSettings();
  });

  it('lists the sections in order and shows the first one when none is picked', () => {
    render(<SettingsPage />);
    const labels = within(nav())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(labels).toEqual(['Proxy', 'Secret Managers', 'AI Agents', 'Support', 'About']);
    expect(screen.getByRole('heading', { name: 'Proxy' })).toBeInTheDocument();
    expect(within(nav()).getByRole('button', { name: 'Proxy' })).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('opens at the section it was asked for', () => {
    useSettingsStore.getState().openSettings('about');
    render(<SettingsPage />);
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
  });

  it('switches the panel when a section is clicked', async () => {
    render(<SettingsPage />);
    await userEvent.click(within(nav()).getByRole('button', { name: 'About' }));
    expect(useSettingsStore.getState().section).toBe('about');
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
  });

  it('closes on the Close button', async () => {
    render(<SettingsPage />);
    await userEvent.click(screen.getByRole('button', { name: 'Close settings' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('disables Close and the other sections while locked', () => {
    useSettingsStore.getState().setLocked(true);
    render(<SettingsPage />);
    expect(screen.getByRole('button', { name: 'Close settings' })).toBeDisabled();
    expect(within(nav()).getByRole('button', { name: 'About' })).toBeDisabled();
    expect(within(nav()).getByRole('button', { name: 'Proxy' })).toBeEnabled();
  });

  it('closes on Esc from the page', () => {
    render(<SettingsPage />);
    fireEvent.keyDown(screen.getByRole('region', { name: 'Settings' }), { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('ignores Esc from an input inside the page', () => {
    render(<SettingsPage />);
    const input = document.createElement('input');
    screen.getByRole('region', { name: 'Settings' }).appendChild(input);
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it('ignores Esc from a nested dialog inside the page', () => {
    render(<SettingsPage />);
    const inner = document.createElement('div');
    inner.setAttribute('role', 'dialog');
    screen.getByRole('region', { name: 'Settings' }).appendChild(inner);
    fireEvent.keyDown(inner, { key: 'Escape' });
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it('takes focus when it opens', () => {
    render(<SettingsPage />);
    expect(screen.getByRole('region', { name: 'Settings' })).toHaveFocus();
  });

  it('gives focus back to the previous element when it closes', () => {
    const outside = document.createElement('button');
    document.body.appendChild(outside);
    outside.focus();
    const { unmount } = render(<SettingsPage />);
    expect(outside).not.toHaveFocus();
    unmount();
    expect(outside).toHaveFocus();
    outside.remove();
  });
  it('keeps keys without a modifier from reaching listeners outside the page', () => {
    const spy = vi.fn();
    document.addEventListener('keydown', spy);
    window.addEventListener('keydown', spy);
    render(<SettingsPage />);
    const root = screen.getByRole('region', { name: 'Settings' });
    fireEvent.keyDown(root, { key: 'Delete' });
    fireEvent.keyDown(root, { key: 'Backspace' });
    document.removeEventListener('keydown', spy);
    window.removeEventListener('keydown', spy);
    expect(spy).not.toHaveBeenCalled();
  });

  it('still runs React key handlers inside the page', async () => {
    render(<SettingsPage />);
    const radios = await screen.findAllByRole('radio');
    const first = radios.find((r) => r.getAttribute('aria-checked') === 'true') ?? radios[0];
    first.focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(first).not.toHaveFocus();
    expect(radios).toContain(document.activeElement);
  });

  it('stops blocking plain keys after it unmounts', () => {
    const spy = vi.fn();
    document.addEventListener('keydown', spy);
    const { unmount } = render(<SettingsPage />);
    unmount();
    fireEvent.keyDown(document.body, { key: 'Delete' });
    document.removeEventListener('keydown', spy);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('lets keys with Ctrl, Meta or Alt reach global shortcut listeners', () => {
    const spy = vi.fn();
    document.addEventListener('keydown', spy);
    render(<SettingsPage />);
    const root = screen.getByRole('region', { name: 'Settings' });
    fireEvent.keyDown(root, { key: ',', ctrlKey: true });
    fireEvent.keyDown(root, { key: 'w', metaKey: true });
    fireEvent.keyDown(root, { key: 'x', altKey: true });
    document.removeEventListener('keydown', spy);
    expect(spy).toHaveBeenCalledTimes(3);
  });

  it('does not steal focus from an element the user clicked outside the page', () => {
    const before = document.createElement('button');
    const clicked = document.createElement('button');
    document.body.append(before, clicked);
    before.focus();
    const { unmount } = render(<SettingsPage />);
    clicked.focus();
    unmount();
    expect(clicked).toHaveFocus();
    before.remove();
    clicked.remove();
  });
});
