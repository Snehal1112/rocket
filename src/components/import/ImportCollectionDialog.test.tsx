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
const mockImportOpenApiUrl = vi.fn();
const mockImportOpenApiFile = vi.fn();
vi.mock('@/lib/tauri-api', () => ({
  createWorkspace: vi.fn(),
  switchWorkspace: vi.fn(),
  getAppDataDir: vi.fn(),
  importBruno: vi.fn(),
  importBrunoZip: vi.fn(),
  importPostmanCollection: vi.fn(),
  importPostmanEnvironment: vi.fn(),
  importWsdl: (...args: unknown[]) => mockImportWsdl(...args),
  importOpenApiUrl: (...args: unknown[]) => mockImportOpenApiUrl(...args),
  importOpenApiFile: (...args: unknown[]) => mockImportOpenApiFile(...args),
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
  mockImportOpenApiUrl.mockReset();
  mockImportOpenApiFile.mockReset();
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

const openApiResult = {
  report,
  collection: 'petstore',
  sourceUrl: null,
  warnings: [] as string[],
};

async function openOpenApiTab() {
  await userEvent.click(screen.getByRole('button', { name: 'OpenAPI' }));
}

describe('ImportCollectionDialog OpenAPI source', () => {
  it('offers an OpenAPI option', () => {
    renderDialog();
    expect(screen.getByRole('button', { name: 'OpenAPI' })).toBeInTheDocument();
  });

  it('hides the OpenAPI option when creating a workspace', () => {
    render(<ImportCollectionDialog open onOpenChange={vi.fn()} workspaceId='ws1' createWorkspace />);
    expect(screen.queryByRole('button', { name: 'OpenAPI' })).not.toBeInTheDocument();
  });

  it('disables Import until a link is typed', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    expect(screen.getByRole('button', { name: 'Import' })).toBeEnabled();
  });

  it('imports a pasted link with the default tag layout', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), '  https://example.com/o.json ');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith('https://example.com/o.json', 'tags', 'ws1'),
    );
  });

  it('sends the path layout when Path is chosen', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('radio', { name: 'Path' }));
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith('https://example.com/o.json', 'paths', 'ws1'),
    );
  });

  it('imports a chosen file', async () => {
    mockOpen.mockResolvedValue('/tmp/o.yaml');
    mockImportOpenApiFile.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.click(screen.getByRole('button', { name: 'File' }));
    await userEvent.click(screen.getByRole('button', { name: /choose spec file/i }));
    expect(mockOpen).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [{ name: 'OpenAPI', extensions: ['json', 'yaml', 'yml'] }],
      }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiFile).toHaveBeenCalledWith('/tmp/o.yaml', 'tags', 'ws1'),
    );
  });

  it('shows the error and keeps the link when the import fails', async () => {
    mockImportOpenApiUrl.mockRejectedValue(new Error('private repositories are not supported yet'));
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(await screen.findByText('private repositories are not supported yet')).toBeInTheDocument();
    expect(screen.getByRole('textbox')).toHaveValue('https://example.com/o.json');
  });

  it('lists warnings on the done screen', async () => {
    mockImportOpenApiUrl.mockResolvedValue({
      ...openApiResult,
      warnings: ['This link points at a commit'],
    });
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(await screen.findByText('This link points at a commit')).toBeInTheDocument();
  });
});
