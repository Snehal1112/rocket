import { describe, expect, it } from 'vitest';
import type { GrpcRequest } from '@/lib/tauri-api';
import {
  createDefaultGrpcState,
  createDefaultRequestFor,
  DEFAULT_GRPC_MESSAGE,
  mapGrpcToState,
} from '../pane-utils';
import { buildGrpcSavePayload, toApiGrpcRequest } from '../request-save-mapper';

const saved: GrpcRequest = {
  uid: 'g1',
  name: 'Say Hello',
  url: 'grpcs://api.example.com:443',
  method: 'demo.greeter.v1.Greeter/SayHello',
  methodType: 'server-streaming',
  protoFilePath: 'protos/greeter.proto',
  metadata: [
    { key: 'x-trace', value: 'abc', enabled: true },
    { key: 'x-off', value: '1', enabled: false },
  ],
  messages: [
    { title: 'first', selected: false, content: '{"name":"a"}' },
    { title: 'second', selected: true, content: '{"name":"b"}' },
  ],
  auth: { authType: 'bearer', token: 't' } as GrpcRequest['auth'],
  tags: ['smoke'],
  docs: 'Greets',
  seq: 4,
  description: 'a description',
  scripts: [{ type: 'before-request', code: '// pre' }],
};

describe('mapGrpcToState', () => {
  it('marks the tab grpc and carries the call description', () => {
    const state = mapGrpcToState(saved);
    expect(state.requestType).toBe('grpc');
    expect(state.url).toBe('grpcs://api.example.com:443');
    expect(state.grpc?.method).toBe('demo.greeter.v1.Greeter/SayHello');
    expect(state.grpc?.methodType).toBe('server-streaming');
    expect(state.grpc?.protoFilePath).toBe('protos/greeter.proto');
  });

  it('reuses the header rows for metadata and keeps disabled rows', () => {
    const state = mapGrpcToState(saved);
    expect(state.headers).toHaveLength(2);
    expect(state.headers[1]).toMatchObject({ key: 'x-off', value: '1', enabled: false });
  });

  it('shows the selected saved message first', () => {
    const state = mapGrpcToState(saved);
    expect(state.grpc?.messages.map((m) => m.title)).toEqual(['first', 'second']);
    expect(state.grpc?.activeMessage).toBe(1);
  });

  it('gives a request with no saved message one empty message', () => {
    const state = mapGrpcToState({ ...saved, messages: undefined, metadata: undefined });
    expect(state.grpc?.messages).toHaveLength(1);
    expect(state.grpc?.messages[0].content).toBe(DEFAULT_GRPC_MESSAGE);
    expect(state.grpc?.activeMessage).toBe(0);
    expect(state.headers).toEqual([]);
  });

  it('maps auth, tags and docs like an HTTP request', () => {
    const state = mapGrpcToState(saved);
    expect(state.auth.authType).toBe('bearer');
    expect(state.tags).toEqual(['smoke']);
    expect(state.docs).toBe('Greets');
  });
});

describe('createDefaultRequestFor', () => {
  it('seeds a grpc request with an empty unary call', () => {
    const state = createDefaultRequestFor('grpc');
    expect(state.requestType).toBe('grpc');
    expect(state.grpc?.methodType).toBe('unary');
    expect(state.grpc?.method).toBe('');
    expect(state.grpc?.messages).toHaveLength(1);
    expect(state.grpc?.messages[0].content).toBe(DEFAULT_GRPC_MESSAGE);
  });

  it('leaves the other kinds without grpc state', () => {
    expect(createDefaultRequestFor('http').grpc).toBeUndefined();
    expect(createDefaultRequestFor('websocket').grpc).toBeUndefined();
  });

  it('gives every default state its own message ids', () => {
    expect(createDefaultGrpcState().messages[0].id).not.toBe(
      createDefaultGrpcState().messages[0].id,
    );
  });
});

describe('toApiGrpcRequest', () => {
  it('round-trips a saved request, including the fields the editor never shows', () => {
    const back = toApiGrpcRequest('g1', 'Say Hello', mapGrpcToState(saved));
    expect(back).toMatchObject({
      uid: 'g1',
      name: 'Say Hello',
      url: saved.url,
      method: saved.method,
      methodType: 'server-streaming',
      protoFilePath: 'protos/greeter.proto',
      tags: ['smoke'],
      docs: 'Greets',
      seq: 4,
      description: 'a description',
      scripts: [{ type: 'before-request', code: '// pre' }],
    });
    expect(back.metadata).toEqual(saved.metadata);
    expect(back.messages).toEqual(saved.messages);
  });

  it('marks exactly the active message as selected', () => {
    const state = mapGrpcToState(saved);
    if (state.grpc) state.grpc.activeMessage = 0;
    const back = toApiGrpcRequest('g1', 'Say Hello', state);
    expect(back.messages?.map((m) => m.selected)).toEqual([true, false]);
  });

  it('omits a blank method and a blank proto path', () => {
    const state = createDefaultRequestFor('grpc');
    state.grpc = { ...(state.grpc ?? createDefaultGrpcState()), protoFilePath: '   ' };
    const back = toApiGrpcRequest('g2', 'New', state);
    expect(back.method).toBeUndefined();
    expect(back.protoFilePath).toBeUndefined();
  });

  it('drops a blank-key draft metadata row but keeps a disabled one', () => {
    const state = mapGrpcToState(saved);
    state.headers = [
      ...state.headers,
      { id: 'draft', key: '', value: 'unfinished', enabled: true },
    ];
    const back = toApiGrpcRequest('g1', 'Say Hello', state);
    expect(back.metadata).toHaveLength(2);
    expect(back.metadata?.[1]).toMatchObject({ key: 'x-off', enabled: false });
  });
});

describe('buildGrpcSavePayload', () => {
  it('uses the tab id as the uid and applies the save-to-collection overrides', () => {
    const payload = buildGrpcSavePayload(
      {
        id: 'tab-9',
        title: 'Untitled',
        tabType: 'request',
        request: createDefaultRequestFor('grpc'),
        response: null,
        isDirty: true,
      },
      { name: 'Chosen', fileName: 'chosen.yml' },
    );
    expect(payload.uid).toBe('tab-9');
    expect(payload.name).toBe('Chosen');
    expect(payload.fileName).toBe('chosen.yml');
    expect(payload.methodType).toBe('unary');
  });
});
