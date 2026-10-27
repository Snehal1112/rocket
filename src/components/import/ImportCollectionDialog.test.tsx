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

  it('file_mode_says_the_import_is_not_kept_in_sync', async () => {
    renderDialog();
    await openOpenApiTab();
    await userEvent.click(screen.getByRole('button', { name: 'File' }));
    expect(
      screen.getByText(
        'A spec imported from a file is not kept in sync. Import from a link to get updates.',
      ),
    ).toBeInTheDocument();
  });

  it('the_import_dialog_hint_mentions_localhost', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.getByText(/http:\/\/localhost for a local server/)).toBeInTheDocument();
  });

  it('link_mode_does_not_show_the_note', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.queryByText(/not kept in sync/)).toBeNull();
  });

  it('imports a pasted link with the default tag layout', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), '  https://example.com/o.json ');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith('https://example.com/o.json', 'tags', 'ws1', undefined),
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
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith('https://example.com/o.json', 'paths', 'ws1', undefined),
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
  it('shows notes without calling them skipped', async () => {
    mockImportOpenApiUrl.mockResolvedValue({
      ...openApiResult,
      report: {
        ...report,
        skipped: [1, 2, 3].map(() => ({
          path: 'api-example.yaml',
          reason: { type: 'notFollowed' as const, detail: 'a $ref to definitions.yaml was not loaded' },
        })),
      },
    });
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(await screen.findByRole('button', { name: '1 note' })).toBeInTheDocument();
    expect(screen.queryByText(/skipped/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/notFollowed/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: '1 note' }));
    expect(await screen.findByText(/3 references come from another file/)).toBeInTheDocument();
  });

  it('shows a token field in Link mode and hides it in File mode', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.getByLabelText('Access token (optional)')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Only for private GitHub repositories. Kept in your system keychain, never in the collection.',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'File' }));
    expect(screen.queryByLabelText('Access token (optional)')).not.toBeInTheDocument();
  });

  it('describes private repository support on the tab', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.getByText(/private repositories need a token/i)).toBeInTheDocument();
    expect(screen.queryByText(/not supported yet/i)).not.toBeInTheDocument();
  });

  it('the token field is a password input', async () => {
    renderDialog();
    await openOpenApiTab();
    expect(screen.getByLabelText('Access token (optional)')).toHaveAttribute('type', 'password');
  });

  it('sends the trimmed token with the link', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://github.com/o/r/blob/main/a.yaml');
    await userEvent.type(screen.getByLabelText('Access token (optional)'), '  t0k  ');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith(
        'https://github.com/o/r/blob/main/a.yaml',
        'tags',
        'ws1',
        't0k',
      ),
    );
  });

  it('sends no token when the field is blank', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.type(screen.getByLabelText('Access token (optional)'), '   ');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith(
        'https://example.com/o.json',
        'tags',
        'ws1',
        undefined,
      ),
    );
  });

  it('clears the token when the dialog closes and reopens', async () => {
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByLabelText('Access token (optional)'), 't0k');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Bruno' })).toBeInTheDocument());
    await openOpenApiTab();
    expect(screen.getByLabelText('Access token (optional)')).toHaveValue('');
  });

  it('clears the token when the tab changes', async () => {
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByLabelText('Access token (optional)'), 't0k');
    await userEvent.click(screen.getByRole('button', { name: 'WSDL' }));
    await openOpenApiTab();
    expect(screen.getByLabelText('Access token (optional)')).toHaveValue('');
  });

  it('does not show the token on the done screen', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://github.com/o/r/blob/main/a.yaml');
    await userEvent.type(screen.getByLabelText('Access token (optional)'), 't0k');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(await screen.findByText('Import complete')).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain('t0k');
  });

  it('offers the update check in Link mode only, off by default, with a hint', async () => {
    renderDialog();
    await openOpenApiTab();
    const box = screen.getByRole('checkbox', { name: 'Check for spec updates' });
    expect(box).not.toBeChecked();
    expect(
      screen.getByText('Rocket checks the link every 5 minutes while the app is open.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'File' }));
    expect(screen.queryByRole('checkbox', { name: 'Check for spec updates' })).toBeNull();
  });

  it('sends checkForUpdates true only when ticked', async () => {
    mockImportOpenApiUrl.mockResolvedValue(openApiResult);
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('checkbox', { name: 'Check for spec updates' }));
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await waitFor(() =>
      expect(mockImportOpenApiUrl).toHaveBeenCalledWith(
        'https://example.com/o.json',
        'tags',
        'ws1',
        undefined,
        true,
      ),
    );
  });

  it('resets the update check when the dialog closes and reopens', async () => {
    renderDialog();
    await openOpenApiTab();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Check for spec updates' }));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Bruno' })).toBeInTheDocument());
    await openOpenApiTab();
    expect(screen.getByRole('checkbox', { name: 'Check for spec updates' })).not.toBeChecked();
  });

  it('shows the warning when update checks could not be turned on', async () => {
    mockImportOpenApiUrl.mockResolvedValue({
      ...openApiResult,
      warnings: ['Update checks could not be turned on. Open the collection menu, then OpenAPI sync, then Connection settings.'],
    });
    renderDialog();
    await openOpenApiTab();
    await userEvent.type(screen.getByRole('textbox'), 'https://example.com/o.json');
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(await screen.findByText(/Update checks could not be turned on/)).toBeInTheDocument();
  });
});
