import { cleanup, fireEvent, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import type { OpenApiBodyView, OpenApiSpecSource, OpenApiSchemaView, OpenApiSpecOperation, OpenApiSpecView as SpecViewData } from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSpecView } from '../OpenApiSpecView';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncSpecView: vi.fn(),
    getOpenApiSyncOverview: vi.fn(),
    getOpenApiSyncSpecSource: vi.fn(),
  };
});

// The real Monaco bundle is heavy to load, so its setup and theme hook are stubbed too.
vi.mock('@/components/editor/monaco-setup', () => ({}));
vi.mock('@/components/editor/useMonacoTheme', () => ({
  useMonacoTheme: () => ({ themeName: 'rocket-light', defineThemes: vi.fn(), isDark: false }),
}));

// Monaco needs a real DOM layout, so the editor becomes a plain element here.
const editorMock = vi.hoisted(() => ({
  revealLineInCenter: vi.fn(),
  setPosition: vi.fn(),
  createDecorationsCollection: vi.fn(() => ({ clear: vi.fn() })),
}));

vi.mock('@monaco-editor/react', async () => {
  const { useEffect } = await import('react');
  return {
    default: ({
      value,
      language,
      onMount,
    }: {
      value?: string;
      language?: string;
      onMount?: (editor: typeof editorMock) => void;
    }) => {
      // The real editor calls onMount once after it is created.
      // biome-ignore lint/correctness/useExhaustiveDependencies: Mount only.
      useEffect(() => onMount?.(editorMock), []);
      return (
        <div data-testid='spec-editor' data-language={language}>
          {value}
        </div>
      );
    },
  };
});

// The 500-row tests render many rows, so a busy machine needs more time.
vi.setConfig({ testTimeout: 30000 });

function op(over: Partial<OpenApiSpecOperation> = {}): OpenApiSpecOperation {
  return {
    key: 'listPets',
    method: 'GET',
    path: '/pets',
    summary: 'List pets',
    parameters: [{ name: 'limit', location: 'query', required: false }],
    responses: [
      { status: '200', description: 'A list', bodies: [] },
      { status: '404', description: 'Missing', bodies: [] },
    ],
    requestBody: [],
    security: [],
    isNew: false,
    ...over,
  };
}

function node(over: Partial<OpenApiSchemaView> = {}): OpenApiSchemaView {
  return {
    kind: 'string',
    format: null,
    description: null,
    required: false,
    nullable: false,
    enumValues: [],
    refName: null,
    properties: [],
    items: null,
    variants: [],
    cut: false,
    ...over,
  };
}

function body(schema: OpenApiSchemaView | null, contentType = 'application/json'): OpenApiBodyView {
  return { contentType, required: true, schema };
}

function view(over: Partial<SpecViewData> = {}): SpecViewData {
  return {
    title: 'Pets API',
    version: '2.1',
    fetchedAt: '2026-10-27T10:00:00+00:00',
    state: 'in_sync',
    groups: [
      { tag: 'pets', operations: [op(), op({ key: 'getPet', path: '/pets/{id}', summary: null })] },
      { tag: 'store', operations: [op({ key: 'getInventory', path: '/inventory', summary: 'Stock' })] },
    ],
    servers: [],
    ...over,
  };
}

function show(data: SpecViewData, waiting = 0, source?: Partial<OpenApiSpecSource>) {
  vi.mocked(tauriApi.getOpenApiSyncSpecSource).mockResolvedValue({
    text: 'openapi: 3.0.0\n',
    format: 'yaml',
    sizeBytes: 2400000,
    tooLarge: false,
    ...source,
  });
  vi.mocked(tauriApi.getOpenApiSyncSpecView).mockResolvedValue(data);
  vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
    endpoints: 3,
    inSync: 3,
    changedByYou: 0,
    updatesWaiting: waiting,
    specVersion: '2.1',
    lastSyncedAt: null,
    lastSyncedUpdated: 0,
  });
  const onBack = vi.fn();
  renderWithQuery(<OpenApiSpecView collection='c' onBack={onBack} />);
  return onBack;
}

describe('OpenApiSpecView', () => {
  beforeEach(() => vi.clearAllMocks());

  it('groups_operations_by_tag', async () => {
    show(view());
    expect(await screen.findByText('pets (2)')).toBeInTheDocument();
    expect(screen.getByText('store (1)')).toBeInTheDocument();
    expect(screen.getByText('/inventory')).toBeInTheDocument();
    expect(screen.getByText('Pets API')).toBeInTheDocument();
  });

  it('shows_a_warning_badge_when_updates_wait', async () => {
    show(view({ state: 'updates_waiting' }), 2);
    expect(await screen.findByText('2 updates waiting')).toBeInTheDocument();
    expect(screen.queryByText('In sync')).toBeNull();
  });

  it('shows_an_in_sync_badge', async () => {
    show(view());
    expect(await screen.findByText('In sync')).toBeInTheDocument();
  });

  it('the_redundant_header_sentences_are_gone', async () => {
    show(view({ state: 'updates_waiting', servers: ['https://api.example.com'] }), 2);
    await screen.findByText('2 updates waiting');
    expect(screen.queryByText('Showing the latest spec Rocket fetched.')).toBeNull();
    expect(screen.queryByText('Your collection is in sync with it.')).toBeNull();
    expect(screen.queryByText('2 updates are waiting.')).toBeNull();
    expect(screen.queryByText('Servers')).toBeNull();
  });

  it('marks_new_operations', async () => {
    show(
      view({
        state: 'updates_waiting',
        groups: [{ tag: 'pets', operations: [op({ key: 'createPet', isNew: true })] }],
      }),
      1,
    );
    expect(await screen.findByText('New')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'GET /pets List pets, new' })).toBeInTheDocument();
  });

  it('search_filters_the_rows', async () => {
    show(view());
    await screen.findByText('pets (2)');
    // The resizable group swallows pointer events in jsdom, so the value is set directly.
    fireEvent.change(screen.getByRole('textbox', { name: 'Search operations' }), {
      target: { value: 'stock' },
    });
    expect(screen.getByText('store (1)')).toBeInTheDocument();
    expect(screen.queryByText(/^pets \(/)).toBeNull();
    expect(screen.queryByText('/pets')).toBeNull();
  });

  it('lists_500_per_tag_then_show_more', async () => {
    const many = Array.from({ length: 600 }, (_, i) =>
      op({ key: `op${i}`, path: `/p${i}`, summary: null }),
    );
    show(view({ groups: [{ tag: 'big', operations: many }] }));
    await screen.findByText('big (600)');
    expect(screen.getAllByRole('button', { name: /^GET \/p/ })).toHaveLength(500);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show more (100)' }));
    expect(screen.getAllByRole('button', { name: /^GET \/p/ })).toHaveLength(600);
  });

  it('one_budget_covers_all_tags', async () => {
    const groups = Array.from({ length: 30 }, (_, g) => ({
      tag: `tag${g}`,
      operations: Array.from({ length: 30 }, (_, i) =>
        op({ key: `op${g}-${i}`, path: `/t${g}/p${i}`, summary: null }),
      ),
    }));
    show(view({ groups }));
    await screen.findByText('tag0 (30)');
    // 900 operations in 30 tags: only the first 500 rows are in the page.
    expect(screen.getAllByRole('button', { name: /^GET \/t/ })).toHaveLength(500);
    expect(screen.queryByText('tag29 (30)')).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show more (400)' }));
    expect(screen.getAllByRole('button', { name: /^GET \/t/ })).toHaveLength(900);
    expect(screen.getByText('tag29 (30)')).toBeInTheDocument();
  });

  it('expanding_a_row_shows_parameters_and_responses', async () => {
    show(view());
    const user = userEvent.setup();
    await screen.findByText('pets (2)');
    expect(screen.queryByText('A list')).toBeNull();
    await user.click(screen.getByRole('button', { name: /GET \/pets List pets/ }));
    const params = screen.getByRole('table', { name: 'Parameters' });
    expect(within(params).getByText('limit')).toBeInTheDocument();
    expect(within(params).getByText('query')).toBeInTheDocument();
    const table = screen.getByRole('table', { name: 'Responses' });
    expect(within(table).getByText('200')).toBeInTheDocument();
    expect(within(table).getByText('A list')).toBeInTheDocument();
    expect(within(table).getByText('Missing')).toBeInTheDocument();
  });

  it('summary_text_is_rendered_as_plain_text', async () => {
    show(
      view({
        groups: [
          { tag: 'pets', operations: [op({ summary: '<script>alert(1)</script> **bold**' })] },
        ],
      }),
    );
    expect(await screen.findByText(/<script>alert\(1\)<\/script> \*\*bold\*\*/)).toBeInTheDocument();
    expect(document.querySelector('script')).toBeNull();
  });

  it('no_copy_shows_no_saved_copy_yet', async () => {
    show(view({ state: 'no_copy', groups: [], title: '', version: null, fetchedAt: null }));
    expect(
      await screen.findByText('No saved copy yet. The next check stores one.'),
    ).toBeInTheDocument();
  });

  it('shows_a_fixed_sentence_when_loading_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncSpecView).mockRejectedValue(
      new Error('cannot read /home/me/secret/path'),
    );
    vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
      endpoints: 0,
      inSync: 0,
      changedByYou: 0,
      updatesWaiting: 0,
      specVersion: null,
      lastSyncedAt: null,
      lastSyncedUpdated: 0,
    });
    renderWithQuery(<OpenApiSpecView collection='c' onBack={vi.fn()} />);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Something went wrong with the OpenAPI sync');
    expect(screen.queryByText(/secret/)).toBeNull();
  });

  it('back_calls_the_handler', async () => {
    const onBack = show(view());
    await screen.findByText('pets (2)');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Back' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  async function expand(name: RegExp) {
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name }));
    return user;
  }

  it('an_expanded_operation_shows_the_request_body_schema', async () => {
    const schema = node({
      kind: 'object',
      properties: [{ name: 'name', schema: node({ required: true, description: 'The pet name.' }) }],
    });
    show(view({ groups: [{ tag: 'pets', operations: [op({ requestBody: [body(schema)] })] }] }));
    await expand(/^GET \/pets List pets$/);
    expect(screen.getByRole('heading', { name: 'Request body' })).toBeInTheDocument();
    expect(screen.getByText('application/json')).toBeInTheDocument();
    expect(screen.getByText('name')).toBeInTheDocument();
    expect(screen.getByText('The pet name.')).toBeInTheDocument();
  });

  it('a_response_schema_opens_on_demand', async () => {
    const schema = node({ kind: 'object', properties: [{ name: 'id', schema: node({ kind: 'integer' }) }] });
    show(
      view({
        groups: [
          {
            tag: 'pets',
            operations: [
              op({
                responses: [{ status: '200', description: 'A list', bodies: [body(schema)] }],
              }),
            ],
          },
        ],
      }),
    );
    const user = await expand(/^GET \/pets List pets$/);
    expect(screen.queryByText('id')).toBeNull();
    await user.click(screen.getByRole('button', { name: /Show schema/ }));
    expect(screen.getByText('id')).toBeInTheDocument();
  });

  it('security_none_is_shown', async () => {
    show(view({ groups: [{ tag: 'pets', operations: [op({ security: [] })] }] }));
    await expand(/^GET \/pets List pets$/);
    expect(screen.getByRole('heading', { name: 'Security' })).toBeInTheDocument();
    expect(screen.getByText('None.')).toBeInTheDocument();
  });

  it('security_names_are_listed', async () => {
    show(view({ groups: [{ tag: 'pets', operations: [op({ security: ['bearer', 'key'] })] }] }));
    await expand(/^GET \/pets List pets$/);
    expect(screen.getByText('bearer, key')).toBeInTheDocument();
  });

  it('one_server_is_a_single_muted_line', async () => {
    show(view({ servers: ['https://api.example.com'] }));
    expect(await screen.findByText('Server: https://api.example.com')).toBeInTheDocument();
  });

  it('several_servers_show_a_count_and_the_full_list_in_the_title', async () => {
    show(view({ servers: ['https://api.example.com', 'https://staging.example.com'] }));
    const line = await screen.findByText('Server: https://api.example.com +1 more');
    expect(line).toHaveAttribute(
      'title',
      'https://api.example.com\nhttps://staging.example.com',
    );
  });

  it('shows_the_source_editor_with_the_format_and_size', async () => {
    show(view(), 0, { format: 'json', text: '{}' });
    const editor = await screen.findByTestId('spec-editor');
    expect(editor).toHaveAttribute('data-language', 'json');
    expect(screen.getByText('JSON, 2.3 MB')).toBeInTheDocument();
  });

  it('the_source_pane_shows_loading_while_operations_render', async () => {
    show(view());
    vi.mocked(tauriApi.getOpenApiSyncSpecSource).mockReturnValue(new Promise(() => {}));
    cleanup();
    renderWithQuery(<OpenApiSpecView collection='c2' onBack={vi.fn()} />);
    expect(await screen.findByText('pets (2)')).toBeInTheDocument();
    expect(screen.getByText('Loading the spec source...')).toBeInTheDocument();
  });

  it('a_too_large_source_shows_a_message_instead_of_the_editor', async () => {
    show(view(), 0, { text: null, tooLarge: true });
    expect(
      await screen.findByText(
        'The spec is too large to show here. The operations on the right still work.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByTestId('spec-editor')).toBeNull();
    expect(screen.getByText('pets (2)')).toBeInTheDocument();
  });

  it('a_source_error_shows_a_fixed_sentence', async () => {
    show(view());
    vi.mocked(tauriApi.getOpenApiSyncSpecSource).mockRejectedValue(new Error('cannot read /secret'));
    cleanup();
    renderWithQuery(<OpenApiSpecView collection='c3' onBack={vi.fn()} />);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Something went wrong with the OpenAPI sync');
    expect(screen.queryByText(/secret/)).toBeNull();
    expect(await screen.findByText('pets (2)')).toBeInTheDocument();
  });

  it('no_source_text_shows_only_the_operations', async () => {
    show(view(), 0, { text: null });
    expect(await screen.findByText('pets (2)')).toBeInTheDocument();
    expect(screen.queryByTestId('spec-editor')).toBeNull();
    expect(screen.queryByRole('separator')).toBeNull();
  });

  it('opening_an_operation_reveals_its_line_in_the_source', async () => {
    const text = 'openapi: 3.0.0\npaths:\n  /pets:\n    get:\n      x: 1\n  /inventory:\n    get:\n      x: 1\n';
    show(view(), 0, { text });
    await screen.findByTestId('spec-editor');
    editorMock.revealLineInCenter.mockClear();
    await expand(/^GET \/inventory Stock$/);
    expect(editorMock.revealLineInCenter).toHaveBeenCalledWith(7);
    expect(editorMock.setPosition).toHaveBeenCalledWith({ lineNumber: 7, column: 1 });
    expect(editorMock.createDecorationsCollection).toHaveBeenCalled();
  });

  it('reopening_the_same_operation_reveals_the_line_again', async () => {
    const text = 'paths:\n  /pets:\n    get:\n      x: 1\n';
    show(view(), 0, { text });
    await screen.findByTestId('spec-editor');
    await expand(/^GET \/pets List pets$/);
    await expand(/^GET \/pets List pets$/);
    editorMock.revealLineInCenter.mockClear();
    await expand(/^GET \/pets List pets$/);
    expect(editorMock.revealLineInCenter).toHaveBeenCalledTimes(1);
    expect(editorMock.revealLineInCenter).toHaveBeenCalledWith(3);
  });

  it('closing_an_operation_does_not_reveal_again', async () => {
    const text = 'paths:\n  /pets:\n    get:\n      x: 1\n';
    show(view(), 0, { text });
    await screen.findByTestId('spec-editor');
    await expand(/^GET \/pets List pets$/);
    editorMock.revealLineInCenter.mockClear();
    await expand(/^GET \/pets List pets$/);
    expect(editorMock.revealLineInCenter).not.toHaveBeenCalled();
  });

  it('opening_an_operation_without_source_text_reveals_nothing', async () => {
    show(view(), 0, { text: null });
    await screen.findByText('pets (2)');
    editorMock.revealLineInCenter.mockClear();
    await expand(/^GET \/pets List pets$/);
    expect(editorMock.revealLineInCenter).not.toHaveBeenCalled();
    expect(screen.getByText('Parameters')).toBeInTheDocument();
  });

  it('an_operation_missing_from_the_source_reveals_nothing', async () => {
    show(view(), 0, { text: 'paths:\n  /other:\n    get:\n' });
    await screen.findByTestId('spec-editor');
    editorMock.revealLineInCenter.mockClear();
    await expand(/^GET \/pets List pets$/);
    expect(editorMock.revealLineInCenter).not.toHaveBeenCalled();
  });

  it('an_operation_without_a_body_shows_no_body_section', async () => {
    show(view());
    await expand(/^GET \/pets List pets$/);
    expect(screen.queryByRole('heading', { name: 'Request body' })).toBeNull();
  });
});
