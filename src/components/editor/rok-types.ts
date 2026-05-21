import chaiTypeDefsRaw from './chai-types.d.ts.txt?raw';

export type ScriptPhase = 'pre-request' | 'post-response' | 'tests';

export interface ScriptSnippetItem {
  label: string;
  code: string;
  kind: 'template' | 'expression';
}

export interface ScriptSnippetSubGroup {
  id: string;
  label: string;
  items: ScriptSnippetItem[];
}

export interface ScriptSnippetGroup {
  id: string;
  label: string;
  items?: ScriptSnippetItem[];
  subGroups?: ScriptSnippetSubGroup[];
}

export const ROK_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-tests',
    label: 'Common Tests',
    items: [
      {
        label: 'Status is 200',
        kind: 'template',
        code: `test("Status is 200", () => {\n  expect(res.getStatus()).to.equal(200);\n});`,
      },
      {
        label: 'Status is 2xx',
        kind: 'template',
        code: `test("Status is 2xx", () => {\n  expect(res.getStatus()).to.be.within(200, 299);\n});`,
      },
      {
        label: 'Response time < 200ms',
        kind: 'template',
        code: `test("Response time < 200ms", () => {\n  expect(res.getResponseTime()).to.be.below(200);\n});`,
      },
      {
        label: 'Body has property',
        kind: 'template',
        code: `test("Body has property", () => {\n  const body = res.getBody();\n  expect(body).to.have.property("key");\n});`,
      },
      {
        label: 'Body equals value',
        kind: 'template',
        code: `test("Body equals value", () => {\n  const body = res.getBody();\n  expect(body.key).to.equal("value");\n});`,
      },
      {
        label: 'Body deep equals',
        kind: 'template',
        code: `test("Body deep equals", () => {\n  const body = res.getBody();\n  expect(body).to.deep.equal({ key: "value" });\n});`,
      },
      {
        label: 'Body is array',
        kind: 'template',
        code: `test("Body is array", () => {\n  expect(res.getBody()).to.be.an("array");\n});`,
      },
      {
        label: 'Body is not empty',
        kind: 'template',
        code: `test("Body is not empty", () => {\n  expect(res.getBody()).to.not.be.empty;\n});`,
      },
      {
        label: 'Header exists',
        kind: 'template',
        code: `test("Header exists", () => {\n  expect(res.getHeader("content-type")).to.exist;\n});`,
      },
      {
        label: 'Header equals value',
        kind: 'template',
        code: `test("Header equals value", () => {\n  expect(res.getHeader("content-type")).to.include("application/json");\n});`,
      },
      {
        label: 'Status is 404',
        kind: 'template',
        code: `test("Status is 404", () => {\n  expect(res.getStatus()).to.equal(404);\n});`,
      },
      {
        label: 'Body matches regex',
        kind: 'template',
        code: `test("Body matches regex", () => {\n  const body = res.getBody({ raw: true });\n  expect(body).to.match(/pattern/);\n});`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'res',
        label: 'res.*',
        items: [
          { label: 'res.getStatus()', kind: 'expression', code: 'res.getStatus()' },
          { label: 'res.getStatusText()', kind: 'expression', code: 'res.getStatusText()' },
          { label: 'res.getHeader("name")', kind: 'expression', code: 'res.getHeader("name")' },
          { label: 'res.getHeaders()', kind: 'expression', code: 'res.getHeaders()' },
          { label: 'res.getBody()', kind: 'expression', code: 'res.getBody()' },
          {
            label: 'res.getBody({ raw: true })',
            kind: 'expression',
            code: 'res.getBody({ raw: true })',
          },
          { label: 'res.getResponseTime()', kind: 'expression', code: 'res.getResponseTime()' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          {
            label: 'rok.setVar("key", value)',
            kind: 'expression',
            code: 'rok.setVar("key", value)',
          },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          {
            label: 'rok.setEnvVar("key", value)',
            kind: 'expression',
            code: 'rok.setEnvVar("key", value)',
          },
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          { label: 'rok.getEnvName()', kind: 'expression', code: 'rok.getEnvName()' },
          {
            label: 'rok.interpolate("{{template}}")',
            kind: 'expression',
            code: 'rok.interpolate("{{template}}")',
          },
        ],
      },
      {
        id: 'expect',
        label: 'expect',
        items: [
          // Equality
          { label: '.to.equal(value)', kind: 'expression', code: '.to.equal(value)' },
          { label: '.to.deep.equal(value)', kind: 'expression', code: '.to.deep.equal(value)' },
          { label: '.to.eql(value)', kind: 'expression', code: '.to.eql(value)' },
          // Existence / truthiness
          { label: '.to.exist', kind: 'expression', code: '.to.exist' },
          { label: '.to.be.ok', kind: 'expression', code: '.to.be.ok' },
          { label: '.to.be.true', kind: 'expression', code: '.to.be.true' },
          { label: '.to.be.false', kind: 'expression', code: '.to.be.false' },
          { label: '.to.be.null', kind: 'expression', code: '.to.be.null' },
          { label: '.to.be.undefined', kind: 'expression', code: '.to.be.undefined' },
          // Type
          { label: '.to.be.a("type")', kind: 'expression', code: '.to.be.a("type")' },
          { label: '.to.be.an("array")', kind: 'expression', code: '.to.be.an("array")' },
          {
            label: '.to.be.instanceof(Constructor)',
            kind: 'expression',
            code: '.to.be.instanceof(Constructor)',
          },
          // Numeric
          { label: '.to.be.above(n)', kind: 'expression', code: '.to.be.above(n)' },
          { label: '.to.be.below(n)', kind: 'expression', code: '.to.be.below(n)' },
          { label: '.to.be.within(min, max)', kind: 'expression', code: '.to.be.within(min, max)' },
          {
            label: '.to.be.closeTo(n, delta)',
            kind: 'expression',
            code: '.to.be.closeTo(n, delta)',
          },
          { label: '.to.be.at.least(n)', kind: 'expression', code: '.to.be.at.least(n)' },
          { label: '.to.be.at.most(n)', kind: 'expression', code: '.to.be.at.most(n)' },
          // String / array / object
          { label: '.to.include("str")', kind: 'expression', code: '.to.include("str")' },
          { label: '.to.have.length(n)', kind: 'expression', code: '.to.have.length(n)' },
          {
            label: '.to.have.property("key")',
            kind: 'expression',
            code: '.to.have.property("key")',
          },
          {
            label: '.to.have.property("key", value)',
            kind: 'expression',
            code: '.to.have.property("key", value)',
          },
          {
            label: '.to.have.own.property("key")',
            kind: 'expression',
            code: '.to.have.own.property("key")',
          },
          { label: '.to.have.keys("a", "b")', kind: 'expression', code: '.to.have.keys("a", "b")' },
          { label: '.to.have.members([...])', kind: 'expression', code: '.to.have.members([])' },
          { label: '.to.match(/regex/)', kind: 'expression', code: '.to.match(/regex/)' },
          { label: '.to.be.empty', kind: 'expression', code: '.to.be.empty' },
          { label: '.to.satisfy(fn)', kind: 'expression', code: '.to.satisfy((val) => val > 0)' },
          // Negation
          { label: '.to.not.equal(value)', kind: 'expression', code: '.to.not.equal(value)' },
          {
            label: '.to.not.have.property("key")',
            kind: 'expression',
            code: '.to.not.have.property("key")',
          },
          { label: '.to.not.include("str")', kind: 'expression', code: '.to.not.include("str")' },
          { label: '.to.not.be.null', kind: 'expression', code: '.to.not.be.null' },
          { label: '.to.not.be.undefined', kind: 'expression', code: '.to.not.be.undefined' },
          { label: '.to.not.be.empty', kind: 'expression', code: '.to.not.be.empty' },
          // Function
          { label: '.to.throw()', kind: 'expression', code: '.to.throw()' },
        ],
      },
    ],
  },
];

const ROK_DEFS = `
declare const rok: {
  /** Read a runtime variable set in a previous script. */
  getVar(key: string): unknown;
  /** Set a runtime variable (in-memory, cleared after request). */
  setVar(key: string, value: unknown): void;
  /** Read an environment variable. */
  getEnvVar(key: string): unknown;
  /** Write an environment variable. Pass { persist: true } to save to disk. */
  setEnvVar(key: string, value: unknown, opts?: { persist?: boolean }): void;
  /** Returns true if the environment variable exists. */
  hasEnvVar(key: string): boolean;
  /** Delete an environment variable. */
  deleteEnvVar(key: string): void;
  /** Returns the active environment name. */
  getEnvName(): string | undefined;
  /** Read a collection variable. */
  getCollectionVar(key: string): unknown;
  /** Write a collection variable (persisted to opencollection.yml). */
  setCollectionVar(key: string, value: unknown): void;
  /** Read a global environment variable. */
  getGlobalEnvVar(key: string): unknown;
  /** Write a global environment variable. */
  setGlobalEnvVar(key: string, value: unknown, opts?: { persist?: boolean }): void;
  /** Resolve {{var}} tokens using the current variable context. */
  interpolate(template: string): string;
  runner: {
    /** Jump to the named request in the runner, or pass null to stop. */
    setNextRequest(name: string | null): void;
    /** Skip this request in the runner. */
    skipRequest(): void;
  };
};
`;

const RES_DEFS = `
declare const res: {
  /** Returns the HTTP status code (e.g. 200). */
  getStatus(): number;
  /** Returns the HTTP status text (e.g. "OK"). */
  getStatusText(): string;
  /** Returns the value of a response header (case-insensitive). */
  getHeader(name: string): string | undefined;
  /** Returns all response headers as a key-value record. */
  getHeaders(): Record<string, string>;
  /** Returns the parsed response body. Pass { raw: true } for the raw string. */
  getBody(opts?: { raw?: boolean }): unknown;
  /** Returns the total response time in milliseconds. */
  getResponseTime(): number;
};
`;

const REQ_DEFS = `
declare const req: {
  getUrl(): string;
  setUrl(url: string): void;
  getHost(): string;
  getPath(): string;
  getQueryString(): string;
  getMethod(): string;
  setMethod(method: string): void;
  getName(): string;
  getAuthMode(): string;
  getHeader(name: string): string | undefined;
  getHeaders(): Record<string, string>;
  setHeader(name: string, value: string): void;
  setHeaders(headers: Record<string, string>): void;
  deleteHeader(name: string): void;
  deleteHeaders(names: string[]): void;
  getBody(opts?: { raw?: boolean }): unknown;
  setBody(body: unknown): void;
  getTimeout(): number;
  setTimeout(ms: number): void;
  setMaxRedirects(n: number): void;
  getExecutionMode(): "runner" | "standalone";
  getExecutionPlatform(): "app";
};
`;

// Strip triple-slash reference directives that Monaco cannot resolve at runtime.
const CHAI_TYPE_DEFS = (chaiTypeDefsRaw as string).replace(/\/\/\/\s*<reference[^>]*>\s*\n/g, '');

const TEST_DEFS = `
${CHAI_TYPE_DEFS}

/** Register a named assertion block. Each block runs independently. */
declare function test(name: string, fn: () => void): void;

/** Full Chai expect — chain assertions with .to.equal(), .to.have.property(), .to.match(), etc. */
declare const expect: Chai.ExpectStatic;
`;

/** Returns the Monaco extra-lib `.d.ts` string for the given script phase. */
export function ROK_TYPE_DEFS_FOR_PHASE(phase: ScriptPhase): string {
  switch (phase) {
    case 'pre-request':
      return ROK_DEFS + REQ_DEFS;
    case 'post-response':
      return ROK_DEFS + RES_DEFS;
    case 'tests':
      return ROK_DEFS + RES_DEFS + TEST_DEFS;
  }
}
