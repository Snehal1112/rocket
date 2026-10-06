import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GrpcExecuteInput, GrpcServiceInfo } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { GrpcMethodPicker } from '../GrpcMethodPicker';

vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    'aria-label': label,
  }: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => <input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />,
}));

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => dialog);

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, grpcListServices: vi.fn() };
});

const services: GrpcServiceInfo[] = [
  {
    name: 'demo.greeter.v1.Greeter',
    methods: [
      {
        name: 'SayHello',
        fullName: 'demo.greeter.v1.Greeter/SayHello',
        methodType: 'unary',
        inputType: 'demo.greeter.v1.HelloRequest',
        outputType: 'demo.greeter.v1.HelloReply',
      },
      {
        name: 'Chat',
        fullName: 'demo.greeter.v1.Greeter/Chat',
        methodType: 'bidi-streaming',
        inputType: 'demo.greeter.v1.HelloRequest',
        outputType: 'demo.greeter.v1.HelloReply',
      },
    ],
  },
];

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
beforeAll(() => {
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

const input: GrpcExecuteInput = {
  collection: 'api',
  request: {
    uid: 'g',
    name: 'G',
    url: 'localhost:50051',
    methodType: 'unary',
    auth: { authType: 'none' },
  },
};

function renderPicker(overrides: Partial<React.ComponentProps<typeof GrpcMethodPicker>> = {}) {
  const props = {
    method: '',
    protoFilePath: 'protos/greeter.proto',
    onProtoFilePathChange: vi.fn(),
    onPick: vi.fn(),
    buildInput: () => input,
    sourceKey: 'a',
    ...overrides,
  };
  return { props, ...render(<GrpcMethodPicker {...props} />) };
}

describe('GrpcMethodPicker', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.grpcListServices).mockReset();
    dialog.open.mockReset();
  });

  it('loads the methods the first time the list is opened and lists them by service', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    renderPicker();
    await userEvent.click(screen.getByLabelText('Method'));

    expect(await screen.findByText('SayHello')).toBeInTheDocument();
    expect(screen.getByText('demo.greeter.v1.Greeter')).toBeInTheDocument();
    expect(screen.getByText('Bidi stream')).toBeInTheDocument();
    expect(tauriApi.grpcListServices).toHaveBeenCalledWith(input, false);
  });

  it('reports the picked method with its call shape', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    const { props } = renderPicker();
    await userEvent.click(screen.getByLabelText('Method'));
    await userEvent.click(await screen.findByText('Chat'));

    expect(props.onPick).toHaveBeenCalledWith(
      expect.objectContaining({
        fullName: 'demo.greeter.v1.Greeter/Chat',
        methodType: 'bidi-streaming',
      }),
    );
  });

  it('shows the saved method before any list is loaded', () => {
    renderPicker({ method: 'demo.greeter.v1.Greeter/SayHello' });
    expect(screen.getByLabelText('Method')).toHaveTextContent('demo.greeter.v1.Greeter/SayHello');
    expect(tauriApi.grpcListServices).not.toHaveBeenCalled();
  });

  it('reloads with refresh when the reload button is used', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    renderPicker();
    await userEvent.click(screen.getByLabelText('Reload methods'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledWith(input, true));
  });

  it('shows the error when the methods cannot be loaded', async () => {
    vi.mocked(tauriApi.grpcListServices).mockRejectedValue(
      new Error('the server does not support gRPC server reflection'),
    );
    renderPicker({ protoFilePath: '' });
    await userEvent.click(screen.getByLabelText('Reload methods'));
    expect(await screen.findByRole('alert')).toHaveTextContent('does not support');
  });

  it('says methods come from reflection when no proto file is set', () => {
    renderPicker({ protoFilePath: '' });
    expect(screen.getByText(/server reflection on the URL/)).toBeInTheDocument();
  });

  it('drops a loaded list when the source changes', async () => {
    vi.mocked(tauriApi.grpcListServices).mockResolvedValue(services);
    const { rerender, props } = renderPicker();
    await userEvent.click(screen.getByLabelText('Reload methods'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledTimes(1));

    rerender(<GrpcMethodPicker {...props} sourceKey='b' />);
    await userEvent.click(screen.getByLabelText('Method'));
    await waitFor(() => expect(tauriApi.grpcListServices).toHaveBeenCalledTimes(2));
  });

  it('puts the chosen file in the proto path', async () => {
    dialog.open.mockResolvedValue('/work/protos/greeter.proto');
    const { props } = renderPicker({ protoFilePath: '' });
    await userEvent.click(screen.getByLabelText('Browse for a proto file'));
    await waitFor(() =>
      expect(props.onProtoFilePathChange).toHaveBeenCalledWith('/work/protos/greeter.proto'),
    );
    expect(dialog.open).toHaveBeenCalledWith(
      expect.objectContaining({ filters: [{ name: 'Protocol Buffers', extensions: ['proto'] }] }),
    );
  });
});
