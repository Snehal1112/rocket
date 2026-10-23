import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ExportCollectionDialog } from './ExportCollectionDialog';

const mockExport = vi.fn();
const mockExportPostman = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  exportCollectionBruno: (...a: unknown[]) => mockExport(...a),
  exportCollectionPostman: (...a: unknown[]) => mockExportPostman(...a),
}));

const mockSaveBinary = vi.fn();
vi.mock('@/lib/save-file', async () => {
  const actual = await vi.importActual<typeof import('@/lib/save-file')>('@/lib/save-file');
  return { ...actual, saveBinaryFile: (...a: unknown[]) => mockSaveBinary(...a) };
});

const mockToastError = vi.fn();
vi.mock('sonner', () => ({
  toast: { error: (...a: unknown[]) => mockToastError(...a), success: vi.fn() },
}));

const result = {
  dataBase64: 'AQI=',
  fileName: 'calc.zip',
  report: {
    exported: 3,
    items: [
      { path: 'a.yml', feature: 'Bearer token', action: 'blanked' },
      { path: 'b.yml', feature: 'Rocket settings', action: 'skipped', detail: 'not in Bruno' },
    ],
  },
};

function renderDialog() {
  return render(<ExportCollectionDialog open collectionName='calc' onClose={vi.fn()} />);
}

beforeEach(() => {
  mockExport.mockReset();
  mockExportPostman.mockReset();
  mockSaveBinary.mockReset();
  mockToastError.mockReset();
});

describe('ExportCollectionDialog', () => {
  it('renders the format choice and an Include environments checkbox on by default', () => {
    renderDialog();
    expect(screen.getByText('Bruno collection (ZIP)')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /include environments/i })).toBeChecked();
  });

  it('export calls exportCollectionBruno with the collection name and the checkbox value, then saves the decoded bytes under the returned file name', async () => {
    mockExport.mockResolvedValue(result);
    mockSaveBinary.mockResolvedValue(true);
    renderDialog();
    await userEvent.click(screen.getByRole('checkbox', { name: /include environments/i }));
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockSaveBinary).toHaveBeenCalled());
    expect(mockExport).toHaveBeenCalledWith('calc', false);
    const [name, bytes] = mockSaveBinary.mock.calls[0];
    expect(name).toBe('calc.zip');
    expect(Array.from(bytes as Uint8Array)).toEqual([1, 2]);
  });

  it('says that bodies, scripts and descriptions are not scanned for secrets', () => {
    renderDialog();
    expect(
      screen.getByText(/bodies, scripts and descriptions are exported as written/i),
    ).toBeInTheDocument();
  });

  it('states which values are blanked and which are exported as written', () => {
    renderDialog();
    const text = screen.getByText(/are blanked/i).textContent ?? '';
    for (const name of [
      'Authorization',
      'URL passwords',
      'query parameters',
      'form fields',
      'OAuth2',
      'variables',
      'secret word such as token',
      'api key',
    ]) {
      expect(text).toContain(name);
    }
    expect(text).toMatch(/bodies, scripts and descriptions are exported as written/);
  });

  it('shows the report items grouped by action after a successful export', async () => {
    mockExport.mockResolvedValue(result);
    mockSaveBinary.mockResolvedValue(true);
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    expect(await screen.findByText('Bearer token')).toBeInTheDocument();
    expect(screen.getByText('Rocket settings')).toBeInTheDocument();
    expect(screen.getByText(/blanked \(1\)/i)).toBeInTheDocument();
    expect(screen.getByText(/skipped \(1\)/i)).toBeInTheDocument();
  });

  it('shows an error toast and no report when the export fails', async () => {
    mockExport.mockRejectedValue(new Error('boom'));
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockToastError).toHaveBeenCalled());
    expect(screen.queryByText(/blanked \(/i)).not.toBeInTheDocument();
  });

  it('does not show a report when the save dialog is cancelled', async () => {
    mockExport.mockResolvedValue(result);
    mockSaveBinary.mockResolvedValue(false);
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockSaveBinary).toHaveBeenCalled());
    expect(screen.queryByText('Bearer token')).not.toBeInTheDocument();
  });

  async function selectPostman() {
    // jsdom lacks these pointer APIs that Radix Select uses.
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.setPointerCapture ??= () => {};
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByRole('option', { name: /postman collection/i }));
  }

  it('postman option calls exportCollectionPostman with both options and saves a .json file', async () => {
    mockExportPostman.mockResolvedValue({ ...result, fileName: 'demo.postman_collection.json' });
    mockSaveBinary.mockResolvedValue(true);
    render(<ExportCollectionDialog open collectionName='demo' onClose={vi.fn()} />);
    await selectPostman();
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockSaveBinary).toHaveBeenCalled());
    expect(mockExportPostman).toHaveBeenCalledWith('demo', {
      preserveScripts: false,
      includeEnvironments: true,
    });
    const [name, , filters] = mockSaveBinary.mock.calls[0];
    expect(name).toMatch(/\.postman_collection\.json$/);
    expect(filters).toEqual([{ name: 'JSON file', extensions: ['json'] }]);
  });

  it('passes Preserve scripts and Include environments to the Postman export', async () => {
    mockExportPostman.mockResolvedValue({ ...result, fileName: 'demo.postman_collection.json' });
    mockSaveBinary.mockResolvedValue(true);
    render(<ExportCollectionDialog open collectionName='demo' onClose={vi.fn()} />);
    await selectPostman();
    await userEvent.click(screen.getByRole('checkbox', { name: /preserve scripts/i }));
    await userEvent.click(screen.getByRole('checkbox', { name: /include environments/i }));
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockExportPostman).toHaveBeenCalled());
    expect(mockExportPostman).toHaveBeenCalledWith('demo', {
      preserveScripts: true,
      includeEnvironments: false,
    });
  });

  it('hides Preserve scripts for the Bruno ZIP format', () => {
    renderDialog();
    expect(screen.queryByRole('checkbox', { name: /preserve scripts/i })).toBeNull();
  });

  it('no longer says Postman environments come in a later version', async () => {
    renderDialog();
    await selectPostman();
    expect(screen.queryByText(/environments are exported in a later version/i)).toBeNull();
    expect(screen.getByRole('checkbox', { name: /include environments/i })).toBeChecked();
  });

  it('saves a ZIP with a zip filter when the Postman result file name ends with .zip', async () => {
    mockExportPostman.mockResolvedValue({ ...result, fileName: 'demo-postman.zip' });
    mockSaveBinary.mockResolvedValue(true);
    render(<ExportCollectionDialog open collectionName='demo' onClose={vi.fn()} />);
    await selectPostman();
    await userEvent.click(screen.getByRole('button', { name: /^export$/i }));
    await waitFor(() => expect(mockSaveBinary).toHaveBeenCalled());
    const [name, , filters] = mockSaveBinary.mock.calls[0];
    expect(name).toBe('demo-postman.zip');
    expect(filters).toEqual([{ name: 'ZIP archive', extensions: ['zip'] }]);
  });
});
