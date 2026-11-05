import { getVersion } from '@tauri-apps/api/app';
import { arch, platform, version } from '@tauri-apps/plugin-os';
import { openUrl } from '@tauri-apps/plugin-opener';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getAppDataDir } from '@/lib/tauri-api';
import { AboutSection } from '../AboutSection';

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: vi.fn(), version: vi.fn(), arch: vi.fn() }));
vi.mock('@/lib/tauri-api', () => ({ getAppDataDir: vi.fn() }));
vi.mock('../UpdatesBlock', () => ({ UpdatesBlock: () => null }));

describe('AboutSection', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(getVersion).mockResolvedValue('1.2.3');
    vi.mocked(getAppDataDir).mockResolvedValue('/home/u/.rocket-api');
    vi.mocked(platform).mockReturnValue('linux');
    vi.mocked(version).mockReturnValue('6.1');
    vi.mocked(arch).mockReturnValue('x86_64');
  });

  it('shows the version, the data folder and the platform', async () => {
    render(<AboutSection />);
    expect(screen.getByRole('heading', { name: 'About' })).toBeInTheDocument();
    expect(await screen.findByText('1.2.3')).toBeInTheDocument();
    expect(await screen.findByText('/home/u/.rocket-api')).toBeInTheDocument();
    expect(screen.getByText('linux 6.1 (x86_64)')).toBeInTheDocument();
  });

  it('shows the about text, the license and the copyright', async () => {
    render(<AboutSection />);
    expect(screen.getByText(/native desktop API client for HTTP, GraphQL/)).toBeInTheDocument();
    expect(await screen.findByText('MIT')).toBeInTheDocument();
    expect(screen.getByText('© 2026 Snehal Dangroshiya')).toBeInTheDocument();
  });

  it('opens the website in the browser', async () => {
    render(<AboutSection />);
    fireEvent.click(screen.getByRole('button', { name: /web\.rocket\.numericlabs\.io/ }));
    await waitFor(() => expect(openUrl).toHaveBeenCalledWith('https://web.rocket.numericlabs.io'));
  });

  it('shows Unknown for a value it cannot read', async () => {
    vi.mocked(getVersion).mockRejectedValue(new Error('no'));
    vi.mocked(platform).mockImplementation(() => {
      throw new Error('no');
    });
    render(<AboutSection />);
    expect(await screen.findByText('/home/u/.rocket-api')).toBeInTheDocument();
    expect(await screen.findAllByText('Unknown')).toHaveLength(2);
  });
});
