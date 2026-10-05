import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ImportCollectionDialog } from './ImportCollectionDialog';
import { describeSource } from './importSources';

const mockOpen = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}));

const mockImportWsdl = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  createWorkspace: vi.fn(),
  switchWorkspace: vi.fn(),
  getAppDataDir: vi.fn(),
  importBruno: vi.fn(),
  importBrunoZip: vi.fn(),
  importPostmanCollection: vi.fn(),
  importPostmanEnvironment: vi.fn(),
  importWsdl: (...args: unknown[]) => mockImportWsdl(...args),
}));

const report = {
  totalFiles: 4,
  imported: 4,
  skipped: [],
  createdWorkspace: null,
  createdCollections: ['calc'],
  detectedType: 'collection' as const,
};

function renderDialog() {
  return render(<ImportCollectionDialog open onOpenChange={vi.fn()} workspaceId='ws1' />);
}

beforeEach(() => {
  mockOpen.mockReset();
  mockImportWsdl.mockReset();
});

describe('describeSource', () => {
  it('labels a WSDL file', () => {
    expect(describeSource('wsdl-file')).toBe('WSDL file');
  });
});

describe('ImportCollectionDialog WSDL source', () => {
  it('offers a WSDL option next to Bruno and Postman', () => {
    renderDialog();
    expect(screen.getByRole('button', { name: 'Bruno' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Postman' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'WSDL' })).toBeInTheDocument();
  });

  it('explains what WSDL import does', async () => {
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    expect(screen.getByText(/one request per SOAP operation/i)).toBeInTheDocument();
  });

  it('keeps Import disabled until a file is chosen, then imports it', async () => {
    mockOpen.mockResolvedValue('/tmp/calc.wsdl');
    mockImportWsdl.mockResolvedValue(report);
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();

    await userEvent.click(screen.getByRole('button', { name: /choose WSDL file/i }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled());
    expect(mockOpen).toHaveBeenCalledWith(
      expect.objectContaining({
        directory: false,
        filters: [{ name: 'WSDL', extensions: ['wsdl', 'xml'] }],
      }),
    );

    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() => expect(mockImportWsdl).toHaveBeenCalledWith('/tmp/calc.wsdl', 'ws1'));
    expect(await screen.findByText(/4 of 4 requests imported/i)).toBeInTheDocument();
  });

  it('shows the backend error and stays on the picker', async () => {
    mockOpen.mockResolvedValue('/tmp/bad.wsdl');
    mockImportWsdl.mockRejectedValue(new Error('not a WSDL 1.1 document'));
    renderDialog();
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    await userEvent.click(screen.getByRole('button', { name: /choose WSDL file/i }));
    await userEvent.click(await screen.findByRole('button', { name: 'Import' }));
    expect(await screen.findByText(/not a WSDL 1.1 document/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import' })).toBeInTheDocument();
  });
});
