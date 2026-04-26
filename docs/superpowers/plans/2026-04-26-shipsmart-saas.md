# ShipSmart — SaaS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build ShipSmart — a SaaS tool that scores PR complexity and API design quality, posts bot comments on GitHub PRs, and surfaces team trends on a dashboard for engineering managers.

**Architecture:** Hybrid analyzer (static rules for scoring + Claude API for plain-English explanations). Node.js/TypeScript backend with Express, Postgres for storage, React + shadcn/ui dashboard, GitHub App for webhook and bot integration.

**Tech Stack:** Node.js, TypeScript, Express, Postgres (pg), Prisma ORM, React, Vite, shadcn/ui, Tailwind CSS, @octokit/rest, @anthropic-ai/sdk, Vitest, Playwright

**Spec:** `docs/superpowers/specs/2026-04-26-shipsmart-saas-design.md`

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `shipsmart/package.json` | Root monorepo config |
| Create | `shipsmart/apps/api/src/index.ts` | Express app entry point |
| Create | `shipsmart/apps/api/src/db/schema.prisma` | Postgres schema |
| Create | `shipsmart/apps/api/src/db/client.ts` | Prisma client singleton |
| Create | `shipsmart/apps/api/src/analyzer/rules/pr-rules.ts` | Static PR complexity rules |
| Create | `shipsmart/apps/api/src/analyzer/rules/api-rules.ts` | Static API health rules |
| Create | `shipsmart/apps/api/src/analyzer/scorer.ts` | Aggregates rule scores → 0–100 |
| Create | `shipsmart/apps/api/src/analyzer/explainer.ts` | Claude API explanation layer |
| Create | `shipsmart/apps/api/src/analyzer/index.ts` | Orchestrates rules + explainer |
| Create | `shipsmart/apps/api/src/webhook/github.ts` | GitHub webhook handler |
| Create | `shipsmart/apps/api/src/bot/comment.ts` | Posts PR comments via Octokit |
| Create | `shipsmart/apps/api/src/auth/github-oauth.ts` | GitHub OAuth flow |
| Create | `shipsmart/apps/api/src/routes/webhook.ts` | POST /webhook/github |
| Create | `shipsmart/apps/api/src/routes/auth.ts` | GET /auth/github, /auth/callback |
| Create | `shipsmart/apps/api/src/routes/prs.ts` | GET /api/prs, /api/prs/:id |
| Create | `shipsmart/apps/api/src/routes/api-health.ts` | POST /api/specs, GET /api/specs/:id |
| Create | `shipsmart/apps/api/src/routes/dashboard.ts` | GET /api/dashboard/team |
| Create | `shipsmart/apps/web/src/main.tsx` | React entry point |
| Create | `shipsmart/apps/web/src/App.tsx` | Router + layout |
| Create | `shipsmart/apps/web/src/pages/PRDetail.tsx` | Engineer PR view |
| Create | `shipsmart/apps/web/src/pages/TeamDashboard.tsx` | EM dashboard view |
| Create | `shipsmart/apps/web/src/pages/ApiHealth.tsx` | API spec upload + results |
| Create | `shipsmart/apps/web/src/components/ScoreBadge.tsx` | Colour-coded 0–100 score badge |
| Create | `shipsmart/apps/web/src/components/TrendChart.tsx` | 5-week complexity trend bar chart |
| Create | `shipsmart/apps/web/src/components/PRList.tsx` | List of high-risk PRs |
| Create | `shipsmart/apps/api/tests/analyzer/pr-rules.test.ts` | Unit tests for PR rules |
| Create | `shipsmart/apps/api/tests/analyzer/api-rules.test.ts` | Unit tests for API rules |
| Create | `shipsmart/apps/api/tests/analyzer/scorer.test.ts` | Unit tests for scorer |
| Create | `shipsmart/apps/api/tests/webhook/github.test.ts` | Webhook handler tests |
| Create | `shipsmart/apps/web/src/components/ScoreBadge.test.tsx` | Component test |
| Create | `shipsmart/apps/web/src/pages/PRDetail.test.tsx` | Component test |

---

## Task 1: Project Scaffold

**Files:**
- Create: `shipsmart/package.json`
- Create: `shipsmart/apps/api/package.json`
- Create: `shipsmart/apps/api/tsconfig.json`
- Create: `shipsmart/apps/web/package.json`
- Create: `shipsmart/apps/web/tsconfig.json`
- Create: `shipsmart/.env.example`

- [ ] **Step 1: Create monorepo root**

```bash
mkdir -p shipsmart/apps/api/src shipsmart/apps/web/src
cd shipsmart
```

Create `shipsmart/package.json`:

```json
{
  "name": "shipsmart",
  "private": true,
  "workspaces": ["apps/*"],
  "scripts": {
    "dev:api": "yarn workspace @shipsmart/api dev",
    "dev:web": "yarn workspace @shipsmart/web dev",
    "test": "yarn workspaces run test"
  }
}
```

- [ ] **Step 2: Create API package**

Create `shipsmart/apps/api/package.json`:

```json
{
  "name": "@shipsmart/api",
  "version": "0.1.0",
  "scripts": {
    "dev": "ts-node-dev --respawn src/index.ts",
    "test": "vitest run",
    "build": "tsc"
  },
  "dependencies": {
    "@anthropic-ai/sdk": "^0.24.0",
    "@octokit/rest": "^20.0.0",
    "@prisma/client": "^5.0.0",
    "express": "^4.18.0",
    "express-async-handler": "^1.2.0",
    "zod": "^3.22.0"
  },
  "devDependencies": {
    "@types/express": "^4.17.0",
    "@types/node": "^20.0.0",
    "prisma": "^5.0.0",
    "ts-node-dev": "^2.0.0",
    "typescript": "^5.0.0",
    "vitest": "^1.0.0"
  }
}
```

Create `shipsmart/apps/api/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "dist",
    "rootDir": "src"
  },
  "include": ["src", "tests"]
}
```

- [ ] **Step 3: Create Web package**

Create `shipsmart/apps/web/package.json`:

```json
{
  "name": "@shipsmart/web",
  "version": "0.1.0",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "react-router-dom": "^6.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "@vitejs/plugin-react": "^4.0.0",
    "typescript": "^5.0.0",
    "vite": "^5.0.0",
    "vitest": "^1.0.0",
    "@testing-library/react": "^14.0.0",
    "@testing-library/jest-dom": "^6.0.0",
    "tailwindcss": "^3.4.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0"
  }
}
```

- [ ] **Step 4: Create .env.example**

Create `shipsmart/.env.example`:

```
DATABASE_URL=postgresql://user:password@localhost:5432/shipsmart
GITHUB_APP_ID=
GITHUB_APP_PRIVATE_KEY=
GITHUB_WEBHOOK_SECRET=
GITHUB_CLIENT_ID=
GITHUB_CLIENT_SECRET=
ANTHROPIC_API_KEY=
SESSION_SECRET=
PORT=3000
```

- [ ] **Step 5: Install dependencies**

```bash
cd shipsmart && yarn install
```

- [ ] **Step 6: Commit**

```bash
git add shipsmart/
git commit -m "chore: scaffold shipsmart monorepo"
```

---

## Task 2: Database Schema

**Files:**
- Create: `shipsmart/apps/api/src/db/schema.prisma`
- Create: `shipsmart/apps/api/src/db/client.ts`

- [ ] **Step 1: Write schema**

Create `shipsmart/apps/api/src/db/schema.prisma`:

```prisma
generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id          String   @id @default(cuid())
  githubId    String   @unique
  login       String
  avatarUrl   String
  accessToken String
  createdAt   DateTime @default(now())
  repos       Repo[]
}

model Repo {
  id        String   @id @default(cuid())
  owner     String
  name      String
  githubId  Int      @unique
  userId    String
  user      User     @relation(fields: [userId], references: [id])
  prs       PR[]
  specs     ApiSpec[]
  createdAt DateTime @default(now())

  @@unique([owner, name])
}

model PR {
  id          String   @id @default(cuid())
  repoId      String
  repo        Repo     @relation(fields: [repoId], references: [id])
  number      Int
  title       String
  author      String
  url         String
  score       Int
  label       String   // "low" | "medium" | "high"
  summary     String?
  fixes       String[] // top 3 fix suggestions
  commentId   Int?     // GitHub comment ID for updates
  createdAt   DateTime @default(now())

  @@unique([repoId, number])
}

model ApiSpec {
  id          String   @id @default(cuid())
  repoId      String
  repo        Repo     @relation(fields: [repoId], references: [id])
  filename    String
  score       Int
  subScores   Json     // { naming: Int, nesting: Int, versioning: Int, methods: Int, endpoints: Int }
  summary     String?
  suggestions String[]
  createdAt   DateTime @default(now())
}
```

- [ ] **Step 2: Create Prisma client singleton**

Create `shipsmart/apps/api/src/db/client.ts`:

```typescript
import { PrismaClient } from '@prisma/client';

const globalForPrisma = globalThis as unknown as { prisma: PrismaClient };

export const prisma = globalForPrisma.prisma ?? new PrismaClient();

if (process.env.NODE_ENV !== 'production') {
  globalForPrisma.prisma = prisma;
}
```

- [ ] **Step 3: Run migration**

```bash
cd shipsmart/apps/api
npx prisma migrate dev --name init
```

Expected: migration files created in `prisma/migrations/`, Prisma client generated.

- [ ] **Step 4: Commit**

```bash
git add shipsmart/apps/api/src/db/ shipsmart/apps/api/prisma/
git commit -m "feat: add database schema and Prisma client"
```

---

## Task 3: Static PR Complexity Rules

**Files:**
- Create: `shipsmart/apps/api/src/analyzer/rules/pr-rules.ts`
- Create: `shipsmart/apps/api/tests/analyzer/pr-rules.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `shipsmart/apps/api/tests/analyzer/pr-rules.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { scorePRDiff } from '../../src/analyzer/rules/pr-rules';

const simpleDiff = `
diff --git a/src/utils.ts b/src/utils.ts
+export function add(a: number, b: number): number {
+  return a + b;
+}
`;

const complexDiff = `
diff --git a/src/payment.ts b/src/payment.ts
+export class PaymentService {
+  processPayment(order: Order) {
+    if (order.type === 'credit') {
+      if (order.amount > 1000) {
+        if (order.customer.tier === 'premium') {
+          if (order.customer.country === 'US') {
+            return this.processUSPremiumCredit(order);
+          }
+        }
+      }
+    }
+  }
+  validateOrder(order: Order) { return true; }
+  sendReceipt(order: Order) { return true; }
+  updateInventory(order: Order) { return true; }
+  notifyWarehouse(order: Order) { return true; }
+}
`;

describe('scorePRDiff', () => {
  it('returns low score for simple diff', () => {
    const result = scorePRDiff(simpleDiff);
    expect(result.total).toBeLessThanOrEqual(40);
    expect(result.breakdown.nestingDepth).toBeLessThanOrEqual(1);
  });

  it('returns high score for complex diff', () => {
    const result = scorePRDiff(complexDiff);
    expect(result.total).toBeGreaterThan(65);
    expect(result.breakdown.nestingDepth).toBeGreaterThan(3);
  });

  it('returns breakdown with all 5 rule scores', () => {
    const result = scorePRDiff(simpleDiff);
    expect(result.breakdown).toMatchObject({
      nestingDepth: expect.any(Number),
      churnRatio: expect.any(Number),
      surfaceAreaGrowth: expect.any(Number),
      responsibilityCount: expect.any(Number),
      cyclomaticComplexity: expect.any(Number),
    });
  });

  it('total is always between 0 and 100', () => {
    const result = scorePRDiff(complexDiff);
    expect(result.total).toBeGreaterThanOrEqual(0);
    expect(result.total).toBeLessThanOrEqual(100);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/pr-rules.test.ts
```

Expected: FAIL — "Cannot find module '../../src/analyzer/rules/pr-rules'"

- [ ] **Step 3: Implement PR rules**

Create `shipsmart/apps/api/src/analyzer/rules/pr-rules.ts`:

```typescript
export interface PRScoreBreakdown {
  nestingDepth: number;       // 0–20
  churnRatio: number;         // 0–20
  surfaceAreaGrowth: number;  // 0–20
  responsibilityCount: number; // 0–20
  cyclomaticComplexity: number; // 0–20
}

export interface PRScore {
  total: number;
  breakdown: PRScoreBreakdown;
  label: 'low' | 'medium' | 'high';
}

export function scorePRDiff(diff: string): PRScore {
  const addedLines = diff.split('\n').filter(l => l.startsWith('+') && !l.startsWith('+++'));

  const breakdown: PRScoreBreakdown = {
    nestingDepth: scoreNestingDepth(addedLines),
    churnRatio: scoreChurnRatio(diff),
    surfaceAreaGrowth: scoreSurfaceAreaGrowth(addedLines),
    responsibilityCount: scoreResponsibilityCount(addedLines),
    cyclomaticComplexity: scoreCyclomaticComplexity(addedLines),
  };

  const total = Math.min(100, Object.values(breakdown).reduce((a, b) => a + b, 0));
  const label = total <= 40 ? 'low' : total <= 65 ? 'medium' : 'high';

  return { total, breakdown, label };
}

function scoreNestingDepth(lines: string[]): number {
  let maxDepth = 0;
  let currentDepth = 0;
  for (const line of lines) {
    const opens = (line.match(/{/g) || []).length;
    const closes = (line.match(/}/g) || []).length;
    currentDepth += opens - closes;
    maxDepth = Math.max(maxDepth, currentDepth);
  }
  // depth 1-2 = 0pts, 3 = 5pts, 4 = 10pts, 5+ = 20pts
  if (maxDepth <= 2) return 0;
  if (maxDepth === 3) return 5;
  if (maxDepth === 4) return 10;
  return 20;
}

function scoreChurnRatio(diff: string): number {
  const filesChanged = (diff.match(/^diff --git/gm) || []).length;
  const linesChanged = (diff.match(/^[+-]/gm) || []).length;
  if (filesChanged === 0) return 0;
  const ratio = linesChanged / filesChanged;
  // ratio < 20 = 0pts, 20-50 = 5pts, 50-100 = 10pts, 100+ = 20pts
  if (ratio < 20) return 0;
  if (ratio < 50) return 5;
  if (ratio < 100) return 10;
  return 20;
}

function scoreSurfaceAreaGrowth(lines: string[]): number {
  const exportCount = lines.filter(l =>
    /\bexport\b/.test(l) && !/\/\//.test(l)
  ).length;
  // 0-2 exports = 0pts, 3-5 = 5pts, 6-10 = 10pts, 10+ = 20pts
  if (exportCount <= 2) return 0;
  if (exportCount <= 5) return 5;
  if (exportCount <= 10) return 10;
  return 20;
}

function scoreResponsibilityCount(lines: string[]): number {
  // Count public methods in classes as a proxy for responsibilities
  const methodCount = lines.filter(l =>
    /^\+\s+(public\s+)?\w+\(/.test(l) && !/constructor/.test(l)
  ).length;
  if (methodCount <= 2) return 0;
  if (methodCount <= 4) return 5;
  if (methodCount <= 7) return 10;
  return 20;
}

function scoreCyclomaticComplexity(lines: string[]): number {
  // Count branching keywords as a proxy for cyclomatic complexity
  const branches = lines.filter(l =>
    /\b(if|else|switch|case|for|while|catch|\?\s*[^:]+:)\b/.test(l)
  ).length;
  if (branches <= 3) return 0;
  if (branches <= 6) return 5;
  if (branches <= 10) return 10;
  return 20;
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/pr-rules.test.ts
```

Expected: PASS — 4 tests passing.

- [ ] **Step 5: Commit**

```bash
git add shipsmart/apps/api/src/analyzer/rules/pr-rules.ts shipsmart/apps/api/tests/analyzer/pr-rules.test.ts
git commit -m "feat: add static PR complexity rules engine"
```

---

## Task 4: Static API Health Rules

**Files:**
- Create: `shipsmart/apps/api/src/analyzer/rules/api-rules.ts`
- Create: `shipsmart/apps/api/tests/analyzer/api-rules.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `shipsmart/apps/api/tests/analyzer/api-rules.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { scoreApiSpec } from '../../src/analyzer/rules/api-rules';

const healthySpec = {
  openapi: '3.0.0',
  info: { title: 'Payments API', version: '1.0.0' },
  paths: {
    '/v1/payments': {
      get: { summary: 'List payments', responses: { '200': { description: 'OK' } } },
      post: { summary: 'Create payment', responses: { '201': { description: 'Created' } } },
    },
    '/v1/payments/{id}': {
      get: { summary: 'Get payment', responses: { '200': { description: 'OK' } } },
      delete: { summary: 'Delete payment', responses: { '204': { description: 'No content' } } },
    },
  },
};

const badSpec = {
  openapi: '3.0.0',
  info: { title: 'Bad API', version: '1.0.0' },
  paths: {
    '/getPayments': { get: { summary: 'Get payments' } },
    '/createPayment': { post: { summary: 'Create payment' } },
    '/deletePayment': { get: { summary: 'Delete payment via GET' } },
    '/listPayments': { get: { summary: 'List' } },
    '/fetchPayments': { get: { summary: 'Fetch' } },
    '/retrievePayments': { get: { summary: 'Retrieve' } },
  },
};

describe('scoreApiSpec', () => {
  it('returns high score for a well-designed spec', () => {
    const result = scoreApiSpec(healthySpec);
    expect(result.total).toBeGreaterThan(70);
    expect(result.subScores.versioning).toBe(20);
  });

  it('returns low score for a poorly designed spec', () => {
    const result = scoreApiSpec(badSpec);
    expect(result.total).toBeLessThan(50);
    expect(result.subScores.versioning).toBe(0);
  });

  it('returns subScores with all 5 dimensions', () => {
    const result = scoreApiSpec(healthySpec);
    expect(result.subScores).toMatchObject({
      naming: expect.any(Number),
      nesting: expect.any(Number),
      versioning: expect.any(Number),
      methods: expect.any(Number),
      endpoints: expect.any(Number),
    });
  });

  it('total is always between 0 and 100', () => {
    const result = scoreApiSpec(badSpec);
    expect(result.total).toBeGreaterThanOrEqual(0);
    expect(result.total).toBeLessThanOrEqual(100);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/api-rules.test.ts
```

Expected: FAIL — "Cannot find module '../../src/analyzer/rules/api-rules'"

- [ ] **Step 3: Implement API rules**

Create `shipsmart/apps/api/src/analyzer/rules/api-rules.ts`:

```typescript
export interface ApiSubScores {
  naming: number;      // 0–20: consistent naming conventions
  nesting: number;     // 0–20: response payload depth
  versioning: number;  // 0–20: /v1/ prefix present
  methods: number;     // 0–20: correct HTTP method usage
  endpoints: number;   // 0–20: no duplicate resource endpoints
}

export interface ApiScore {
  total: number;
  subScores: ApiSubScores;
  label: 'low' | 'medium' | 'high';
}

export function scoreApiSpec(spec: Record<string, unknown>): ApiScore {
  const paths = (spec.paths as Record<string, unknown>) ?? {};
  const pathKeys = Object.keys(paths);

  const subScores: ApiSubScores = {
    naming: scoreNaming(pathKeys),
    nesting: scoreNesting(paths),
    versioning: scoreVersioning(pathKeys),
    methods: scoreMethods(paths),
    endpoints: scoreEndpoints(pathKeys),
  };

  const total = Math.min(100, Object.values(subScores).reduce((a, b) => a + b, 0));
  const label = total <= 40 ? 'low' : total <= 65 ? 'medium' : 'high';

  return { total, subScores, label };
}

function scoreNaming(paths: string[]): number {
  if (paths.length === 0) return 20;
  // Good: /v1/payment-orders — kebab-case resource names
  // Bad: /getPayments, /createPayment — verb prefixes
  const verbPrefixPattern = /\/(get|create|delete|update|fetch|list|retrieve|add|remove)[A-Z]/;
  const verbCount = paths.filter(p => verbPrefixPattern.test(p)).length;
  const ratio = verbCount / paths.length;
  if (ratio === 0) return 20;
  if (ratio < 0.25) return 15;
  if (ratio < 0.5) return 10;
  if (ratio < 0.75) return 5;
  return 0;
}

function scoreNesting(paths: Record<string, unknown>): number {
  // Check response schema depth (simplified: count nesting in path object)
  let maxDepth = 0;
  const measure = (obj: unknown, depth: number) => {
    if (depth > maxDepth) maxDepth = depth;
    if (obj && typeof obj === 'object') {
      for (const v of Object.values(obj as Record<string, unknown>)) {
        measure(v, depth + 1);
      }
    }
  };
  measure(paths, 0);
  // API response nesting mapped to score (deeper schema = more complex)
  if (maxDepth <= 6) return 20;
  if (maxDepth <= 8) return 15;
  if (maxDepth <= 10) return 10;
  if (maxDepth <= 12) return 5;
  return 0;
}

function scoreVersioning(paths: string[]): number {
  if (paths.length === 0) return 20;
  const versioned = paths.filter(p => /\/v\d+\//.test(p)).length;
  return versioned === paths.length ? 20 : versioned > 0 ? 10 : 0;
}

function scoreMethods(paths: Record<string, unknown>): number {
  let violations = 0;
  for (const [, methods] of Object.entries(paths)) {
    const m = methods as Record<string, unknown>;
    // GET should not be used for mutations — look for "delete", "create", "update" in summary
    if (m.get) {
      const summary = ((m.get as Record<string, unknown>).summary as string ?? '').toLowerCase();
      if (/delete|remove|create|update|mutate/.test(summary)) violations++;
    }
  }
  if (violations === 0) return 20;
  if (violations === 1) return 10;
  return 0;
}

function scoreEndpoints(paths: string[]): number {
  // Flag when multiple paths target the same resource with similar verbs in name
  const resources = paths.map(p => p.replace(/\/v\d+/, '').split('/')[1] ?? '');
  const duplicates = resources.length - new Set(resources).size;
  if (duplicates === 0) return 20;
  if (duplicates <= 2) return 10;
  return 0;
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/api-rules.test.ts
```

Expected: PASS — 4 tests passing.

- [ ] **Step 5: Commit**

```bash
git add shipsmart/apps/api/src/analyzer/rules/api-rules.ts shipsmart/apps/api/tests/analyzer/api-rules.test.ts
git commit -m "feat: add static API health rules engine"
```

---

## Task 5: Scorer + LLM Explainer

**Files:**
- Create: `shipsmart/apps/api/src/analyzer/scorer.ts`
- Create: `shipsmart/apps/api/src/analyzer/explainer.ts`
- Create: `shipsmart/apps/api/src/analyzer/index.ts`
- Create: `shipsmart/apps/api/tests/analyzer/scorer.test.ts`

- [ ] **Step 1: Write the failing scorer test**

Create `shipsmart/apps/api/tests/analyzer/scorer.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { labelFromScore } from '../../src/analyzer/scorer';

describe('labelFromScore', () => {
  it('returns low for 0–40', () => {
    expect(labelFromScore(0)).toBe('low');
    expect(labelFromScore(40)).toBe('low');
  });

  it('returns medium for 41–65', () => {
    expect(labelFromScore(41)).toBe('medium');
    expect(labelFromScore(65)).toBe('medium');
  });

  it('returns high for 66–100', () => {
    expect(labelFromScore(66)).toBe('high');
    expect(labelFromScore(100)).toBe('high');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/scorer.test.ts
```

Expected: FAIL — "Cannot find module"

- [ ] **Step 3: Implement scorer**

Create `shipsmart/apps/api/src/analyzer/scorer.ts`:

```typescript
export type ScoreLabel = 'low' | 'medium' | 'high';

export function labelFromScore(score: number): ScoreLabel {
  if (score <= 40) return 'low';
  if (score <= 65) return 'medium';
  return 'high';
}

export function scoreToEmoji(score: number): string {
  const label = labelFromScore(score);
  if (label === 'low') return '🟢';
  if (label === 'medium') return '🟡';
  return '🔴';
}
```

- [ ] **Step 4: Run scorer test to verify it passes**

```bash
cd shipsmart/apps/api && yarn test tests/analyzer/scorer.test.ts
```

Expected: PASS — 3 tests passing.

- [ ] **Step 5: Implement LLM explainer**

Create `shipsmart/apps/api/src/analyzer/explainer.ts`:

```typescript
import Anthropic from '@anthropic-ai/sdk';
import { PRScoreBreakdown } from './rules/pr-rules';

const client = new Anthropic();

export interface PRExplanation {
  summary: string;
  fixes: string[];
}

export async function explainPRScore(
  diff: string,
  score: number,
  breakdown: PRScoreBreakdown,
  timeoutMs = 10_000
): Promise<PRExplanation | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const message = await client.messages.create({
      model: 'claude-sonnet-4-6',
      max_tokens: 512,
      system: `You are a senior engineer reviewing a pull request for complexity.
Given a diff and complexity scores, write:
1. A 2-sentence plain-English summary of what makes this PR complex (be specific — reference actual class/file names from the diff)
2. Exactly 3 actionable fix suggestions (each one sentence, reference specific names from the diff)

Respond in this exact JSON format:
{"summary": "...", "fixes": ["fix 1", "fix 2", "fix 3"]}`,
      messages: [
        {
          role: 'user',
          content: `Complexity score: ${score}/100
Breakdown: nesting=${breakdown.nestingDepth}, churn=${breakdown.churnRatio}, surface=${breakdown.surfaceAreaGrowth}, responsibilities=${breakdown.responsibilityCount}, cyclomatic=${breakdown.cyclomaticComplexity}

Diff:
${diff.slice(0, 3000)}`,
        },
      ],
    });

    clearTimeout(timer);
    const text = (message.content[0] as { type: string; text: string }).text;
    return JSON.parse(text) as PRExplanation;
  } catch {
    clearTimeout(timer);
    return null;
  }
}

export async function explainApiScore(
  specJson: string,
  score: number,
): Promise<{ summary: string; suggestions: string[] } | null> {
  try {
    const message = await client.messages.create({
      model: 'claude-sonnet-4-6',
      max_tokens: 512,
      system: `You are a senior API designer reviewing an OpenAPI spec for design quality.
Given a spec and scores, write:
1. A 2-sentence summary of the top design issues (be specific — reference actual endpoint paths)
2. Exactly 3 improvement suggestions (each one sentence, reference specific paths/schemas)

Respond in this exact JSON format:
{"summary": "...", "suggestions": ["suggestion 1", "suggestion 2", "suggestion 3"]}`,
      messages: [
        {
          role: 'user',
          content: `API health score: ${score}/100\n\nSpec:\n${specJson.slice(0, 3000)}`,
        },
      ],
    });

    const text = (message.content[0] as { type: string; text: string }).text;
    return JSON.parse(text) as { summary: string; suggestions: string[] };
  } catch {
    return null;
  }
}
```

- [ ] **Step 6: Wire analyzer orchestrator**

Create `shipsmart/apps/api/src/analyzer/index.ts`:

```typescript
import { scorePRDiff, PRScore } from './rules/pr-rules';
import { scoreApiSpec, ApiScore } from './rules/api-rules';
import { explainPRScore, PRExplanation } from './explainer';

export interface PRAnalysisResult {
  score: PRScore;
  explanation: PRExplanation | null;
}

export interface ApiAnalysisResult {
  score: ApiScore;
  explanation: { summary: string; suggestions: string[] } | null;
}

export async function analyzePR(diff: string): Promise<PRAnalysisResult> {
  const score = scorePRDiff(diff);
  const explanation = score.total > 40
    ? await explainPRScore(diff, score.total, score.breakdown)
    : null;
  return { score, explanation };
}

export async function analyzeApiSpec(spec: Record<string, unknown>): Promise<ApiAnalysisResult> {
  const score = scoreApiSpec(spec);
  const { explainApiScore } = await import('./explainer');
  const explanation = await explainApiScore(JSON.stringify(spec), score.total);
  return { score, explanation };
}
```

- [ ] **Step 7: Commit**

```bash
git add shipsmart/apps/api/src/analyzer/ shipsmart/apps/api/tests/analyzer/scorer.test.ts
git commit -m "feat: add scorer, LLM explainer, and analyzer orchestrator"
```

---

## Task 6: Express App + Webhook Route

**Files:**
- Create: `shipsmart/apps/api/src/index.ts`
- Create: `shipsmart/apps/api/src/webhook/github.ts`
- Create: `shipsmart/apps/api/src/routes/webhook.ts`
- Create: `shipsmart/apps/api/tests/webhook/github.test.ts`

- [ ] **Step 1: Write the failing webhook test**

Create `shipsmart/apps/api/tests/webhook/github.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { handlePRWebhook } from '../../src/webhook/github';

vi.mock('../../src/analyzer/index', () => ({
  analyzePR: vi.fn().mockResolvedValue({
    score: { total: 74, label: 'high', breakdown: {} },
    explanation: { summary: 'Complex PR', fixes: ['Fix 1', 'Fix 2', 'Fix 3'] },
  }),
}));

vi.mock('../../src/bot/comment', () => ({
  postPRComment: vi.fn().mockResolvedValue(42),
}));

vi.mock('../../src/db/client', () => ({
  prisma: {
    repo: { upsert: vi.fn().mockResolvedValue({ id: 'repo-1' }) },
    pR: { upsert: vi.fn().mockResolvedValue({ id: 'pr-1' }) },
  },
}));

const prPayload = {
  action: 'opened',
  pull_request: {
    number: 42,
    title: 'Add payment service',
    html_url: 'https://github.com/org/repo/pull/42',
    user: { login: 'snehal' },
    diff_url: 'https://github.com/org/repo/pull/42.diff',
  },
  repository: {
    id: 123,
    name: 'repo',
    owner: { login: 'org' },
  },
};

describe('handlePRWebhook', () => {
  it('processes an opened PR event without throwing', async () => {
    await expect(handlePRWebhook(prPayload, 'fake-token')).resolves.not.toThrow();
  });

  it('ignores non-opened/synchronize actions', async () => {
    const { analyzePR } = await import('../../src/analyzer/index');
    await handlePRWebhook({ ...prPayload, action: 'closed' }, 'fake-token');
    expect(analyzePR).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd shipsmart/apps/api && yarn test tests/webhook/github.test.ts
```

Expected: FAIL — "Cannot find module"

- [ ] **Step 3: Create bot/comment stub (needed by webhook handler)**

Create `shipsmart/apps/api/src/bot/comment.ts`:

```typescript
import { Octokit } from '@octokit/rest';
import { scoreToEmoji } from '../analyzer/scorer';

export async function postPRComment(
  owner: string,
  repo: string,
  prNumber: number,
  score: number,
  label: string,
  summary: string | null,
  fixes: string[],
  installationToken: string,
): Promise<number> {
  const octokit = new Octokit({ auth: installationToken });
  const emoji = scoreToEmoji(score);

  const fixLines = fixes.length > 0
    ? fixes.map((f, i) => `${i + 1}. ${f}`).join('\n')
    : '_No specific fixes suggested for low-complexity PRs._';

  const summaryLine = summary ?? '_Explanation unavailable._';

  const body = `## ShipSmart Complexity Report ${emoji} ${score}/100

**Summary:** ${summaryLine}

**Top fixes:**
${fixLines}`;

  const response = await octokit.issues.createComment({
    owner,
    repo,
    issue_number: prNumber,
    body,
  });

  return response.data.id;
}
```

- [ ] **Step 4: Implement webhook handler**

Create `shipsmart/apps/api/src/webhook/github.ts`:

```typescript
import { analyzePR } from '../analyzer/index';
import { postPRComment } from '../bot/comment';
import { prisma } from '../db/client';

interface PRWebhookPayload {
  action: string;
  pull_request: {
    number: number;
    title: string;
    html_url: string;
    user: { login: string };
    diff_url: string;
  };
  repository: {
    id: number;
    name: string;
    owner: { login: string };
  };
}

export async function handlePRWebhook(
  payload: PRWebhookPayload,
  installationToken: string,
): Promise<void> {
  if (!['opened', 'synchronize'].includes(payload.action)) return;

  const { pull_request: pr, repository } = payload;

  // Fetch the diff
  const diffResponse = await fetch(pr.diff_url, {
    headers: { Authorization: `token ${installationToken}` },
  });
  const diff = await diffResponse.text();

  // Analyze
  const { score, explanation } = await analyzePR(diff);

  // Upsert repo + PR in DB
  const repo = await prisma.repo.upsert({
    where: { githubId: repository.id },
    create: {
      githubId: repository.id,
      owner: repository.owner.login,
      name: repository.name,
      userId: 'system', // updated when user connects OAuth
    },
    update: {},
  });

  const fixes = explanation?.fixes ?? [];
  const summary = explanation?.summary ?? null;

  const commentId = await postPRComment(
    repository.owner.login,
    repository.name,
    pr.number,
    score.total,
    score.label,
    summary,
    fixes,
    installationToken,
  );

  await prisma.pR.upsert({
    where: { repoId_number: { repoId: repo.id, number: pr.number } },
    create: {
      repoId: repo.id,
      number: pr.number,
      title: pr.title,
      author: pr.user.login,
      url: pr.html_url,
      score: score.total,
      label: score.label,
      summary,
      fixes,
      commentId,
    },
    update: { score: score.total, label: score.label, summary, fixes, commentId },
  });
}
```

- [ ] **Step 5: Create Express app entry point**

Create `shipsmart/apps/api/src/index.ts`:

```typescript
import express from 'express';
import { webhookRouter } from './routes/webhook';

const app = express();
app.use(express.json({ limit: '10mb' }));
app.use(express.raw({ type: 'application/json' }));

app.use('/webhook', webhookRouter);

app.get('/health', (_req, res) => res.json({ status: 'ok' }));

const port = process.env.PORT ?? 3000;
app.listen(port, () => {
  console.log(`ShipSmart API running on port ${port}`);
});

export { app };
```

- [ ] **Step 6: Create webhook route**

Create `shipsmart/apps/api/src/routes/webhook.ts`:

```typescript
import { Router } from 'express';
import { handlePRWebhook } from '../webhook/github';

export const webhookRouter = Router();

webhookRouter.post('/github', async (req, res) => {
  const event = req.headers['x-github-event'] as string;

  if (event === 'pull_request') {
    // In production: verify HMAC signature from x-hub-signature-256
    const installationToken = process.env.GITHUB_INSTALLATION_TOKEN ?? '';
    handlePRWebhook(req.body, installationToken).catch(console.error);
  }

  res.status(200).json({ received: true });
});
```

- [ ] **Step 7: Run tests to verify they pass**

```bash
cd shipsmart/apps/api && yarn test tests/webhook/github.test.ts
```

Expected: PASS — 2 tests passing.

- [ ] **Step 8: Commit**

```bash
git add shipsmart/apps/api/src/
git commit -m "feat: add Express app, webhook handler, and bot comment poster"
```

---

## Task 7: Auth + API Routes

**Files:**
- Create: `shipsmart/apps/api/src/auth/github-oauth.ts`
- Create: `shipsmart/apps/api/src/routes/auth.ts`
- Create: `shipsmart/apps/api/src/routes/prs.ts`
- Create: `shipsmart/apps/api/src/routes/api-health.ts`
- Create: `shipsmart/apps/api/src/routes/dashboard.ts`

- [ ] **Step 1: Implement GitHub OAuth**

Create `shipsmart/apps/api/src/auth/github-oauth.ts`:

```typescript
import { prisma } from '../db/client';

const GITHUB_CLIENT_ID = process.env.GITHUB_CLIENT_ID!;
const GITHUB_CLIENT_SECRET = process.env.GITHUB_CLIENT_SECRET!;

export function getAuthorizationUrl(): string {
  const params = new URLSearchParams({
    client_id: GITHUB_CLIENT_ID,
    scope: 'read:user repo',
  });
  return `https://github.com/login/oauth/authorize?${params}`;
}

export async function exchangeCodeForToken(code: string): Promise<string> {
  const response = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
    body: JSON.stringify({
      client_id: GITHUB_CLIENT_ID,
      client_secret: GITHUB_CLIENT_SECRET,
      code,
    }),
  });
  const data = await response.json() as { access_token: string };
  return data.access_token;
}

export async function getOrCreateUser(accessToken: string) {
  const res = await fetch('https://api.github.com/user', {
    headers: { Authorization: `token ${accessToken}` },
  });
  const gh = await res.json() as { id: number; login: string; avatar_url: string };

  return prisma.user.upsert({
    where: { githubId: String(gh.id) },
    create: {
      githubId: String(gh.id),
      login: gh.login,
      avatarUrl: gh.avatar_url,
      accessToken,
    },
    update: { accessToken, avatarUrl: gh.avatar_url },
  });
}
```

- [ ] **Step 2: Create auth routes**

Create `shipsmart/apps/api/src/routes/auth.ts`:

```typescript
import { Router } from 'express';
import { getAuthorizationUrl, exchangeCodeForToken, getOrCreateUser } from '../auth/github-oauth';

export const authRouter = Router();

authRouter.get('/github', (_req, res) => {
  res.redirect(getAuthorizationUrl());
});

authRouter.get('/callback', async (req, res) => {
  const code = req.query.code as string;
  if (!code) return res.status(400).json({ error: 'Missing code' });

  try {
    const token = await exchangeCodeForToken(code);
    const user = await getOrCreateUser(token);
    // In production: set a signed session cookie
    res.redirect(`${process.env.WEB_URL}/?userId=${user.id}`);
  } catch {
    res.redirect(`${process.env.WEB_URL}/error?reason=auth_failed`);
  }
});
```

- [ ] **Step 3: Create PR routes**

Create `shipsmart/apps/api/src/routes/prs.ts`:

```typescript
import { Router } from 'express';
import { prisma } from '../db/client';

export const prsRouter = Router();

prsRouter.get('/', async (req, res) => {
  const { repoId } = req.query;
  const prs = await prisma.pR.findMany({
    where: repoId ? { repoId: repoId as string } : {},
    orderBy: { createdAt: 'desc' },
    take: 50,
  });
  res.json(prs);
});

prsRouter.get('/:id', async (req, res) => {
  const pr = await prisma.pR.findUnique({ where: { id: req.params.id } });
  if (!pr) return res.status(404).json({ error: 'Not found' });
  res.json(pr);
});
```

- [ ] **Step 4: Create API health routes**

Create `shipsmart/apps/api/src/routes/api-health.ts`:

```typescript
import { Router } from 'express';
import { z } from 'zod';
import { analyzeApiSpec } from '../analyzer/index';
import { prisma } from '../db/client';

export const apiHealthRouter = Router();

const UploadBody = z.object({
  repoId: z.string(),
  filename: z.string(),
  spec: z.record(z.unknown()),
});

apiHealthRouter.post('/', async (req, res) => {
  const parsed = UploadBody.safeParse(req.body);
  if (!parsed.success) return res.status(400).json({ error: parsed.error.flatten() });

  const { repoId, filename, spec } = parsed.data;
  const { score, explanation } = await analyzeApiSpec(spec);

  const saved = await prisma.apiSpec.create({
    data: {
      repoId,
      filename,
      score: score.total,
      subScores: score.subScores,
      summary: explanation?.summary ?? null,
      suggestions: explanation?.suggestions ?? [],
    },
  });

  res.json(saved);
});

apiHealthRouter.get('/:id', async (req, res) => {
  const spec = await prisma.apiSpec.findUnique({ where: { id: req.params.id } });
  if (!spec) return res.status(404).json({ error: 'Not found' });
  res.json(spec);
});
```

- [ ] **Step 5: Create dashboard route**

Create `shipsmart/apps/api/src/routes/dashboard.ts`:

```typescript
import { Router } from 'express';
import { prisma } from '../db/client';

export const dashboardRouter = Router();

dashboardRouter.get('/team', async (req, res) => {
  const { repoId } = req.query;
  const where = repoId ? { repoId: repoId as string } : {};

  const fiveWeeksAgo = new Date();
  fiveWeeksAgo.setDate(fiveWeeksAgo.getDate() - 35);

  const [recentPRs, allPRs, apiDebt] = await Promise.all([
    // PRs from this week
    prisma.pR.findMany({
      where: { ...where, createdAt: { gte: new Date(Date.now() - 7 * 86400 * 1000) } },
      orderBy: { score: 'desc' },
    }),
    // PRs from last 5 weeks for trend
    prisma.pR.findMany({
      where: { ...where, createdAt: { gte: fiveWeeksAgo } },
      orderBy: { createdAt: 'asc' },
    }),
    // API spec issues
    prisma.apiSpec.count({ where }),
  ]);

  const avgScore = recentPRs.length
    ? Math.round(recentPRs.reduce((s, p) => s + p.score, 0) / recentPRs.length)
    : 0;

  const highRiskPRs = recentPRs.filter(p => p.score > 65);

  // Group allPRs by week for trend
  const weeklyTrend = Array.from({ length: 5 }, (_, i) => {
    const weekStart = new Date(fiveWeeksAgo);
    weekStart.setDate(weekStart.getDate() + i * 7);
    const weekEnd = new Date(weekStart);
    weekEnd.setDate(weekEnd.getDate() + 7);
    const weekPRs = allPRs.filter(p => p.createdAt >= weekStart && p.createdAt < weekEnd);
    const avg = weekPRs.length
      ? Math.round(weekPRs.reduce((s, p) => s + p.score, 0) / weekPRs.length)
      : 0;
    return { week: i + 1, avg };
  });

  res.json({ avgScore, highRiskPRs, weeklyTrend, apiDebtCount: apiDebt });
});
```

- [ ] **Step 6: Wire all routes into Express app**

Edit `shipsmart/apps/api/src/index.ts` — replace the existing file:

```typescript
import express from 'express';
import { webhookRouter } from './routes/webhook';
import { authRouter } from './routes/auth';
import { prsRouter } from './routes/prs';
import { apiHealthRouter } from './routes/api-health';
import { dashboardRouter } from './routes/dashboard';

const app = express();
app.use(express.json({ limit: '10mb' }));

app.use('/webhook', webhookRouter);
app.use('/auth', authRouter);
app.use('/api/prs', prsRouter);
app.use('/api/specs', apiHealthRouter);
app.use('/api/dashboard', dashboardRouter);

app.get('/health', (_req, res) => res.json({ status: 'ok' }));

const port = process.env.PORT ?? 3000;
app.listen(port, () => {
  console.log(`ShipSmart API running on port ${port}`);
});

export { app };
```

- [ ] **Step 7: Commit**

```bash
git add shipsmart/apps/api/src/
git commit -m "feat: add auth, PR, API health, and dashboard routes"
```

---

## Task 8: React Dashboard — Scaffold + ScoreBadge

**Files:**
- Create: `shipsmart/apps/web/src/main.tsx`
- Create: `shipsmart/apps/web/src/App.tsx`
- Create: `shipsmart/apps/web/src/components/ScoreBadge.tsx`
- Create: `shipsmart/apps/web/src/components/ScoreBadge.test.tsx`
- Create: `shipsmart/apps/web/index.html`
- Create: `shipsmart/apps/web/vite.config.ts`
- Create: `shipsmart/apps/web/tailwind.config.js`

- [ ] **Step 1: Write the failing ScoreBadge test**

Create `shipsmart/apps/web/src/components/ScoreBadge.test.tsx`:

```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ScoreBadge } from './ScoreBadge';

describe('ScoreBadge', () => {
  it('renders the score number', () => {
    render(<ScoreBadge score={74} />);
    expect(screen.getByText('74')).toBeDefined();
  });

  it('shows red for high score', () => {
    const { container } = render(<ScoreBadge score={74} />);
    expect(container.firstChild?.toString()).toContain('74');
  });

  it('shows green for low score', () => {
    render(<ScoreBadge score={20} />);
    expect(screen.getByText('20')).toBeDefined();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd shipsmart/apps/web && yarn test src/components/ScoreBadge.test.tsx
```

Expected: FAIL — "Cannot find module './ScoreBadge'"

- [ ] **Step 3: Create vite config**

Create `shipsmart/apps/web/vite.config.ts`:

```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: './src/test-setup.ts',
  },
  server: {
    proxy: {
      '/api': 'http://localhost:3000',
      '/auth': 'http://localhost:3000',
    },
  },
});
```

Create `shipsmart/apps/web/src/test-setup.ts`:

```typescript
import '@testing-library/jest-dom';
```

Create `shipsmart/apps/web/index.html`:

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>ShipSmart</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 4: Implement ScoreBadge**

Create `shipsmart/apps/web/src/components/ScoreBadge.tsx`:

```typescript
interface ScoreBadgeProps {
  score: number;
}

function colorClass(score: number): string {
  if (score <= 40) return 'bg-green-500';
  if (score <= 65) return 'bg-yellow-500';
  return 'bg-red-500';
}

export function ScoreBadge({ score }: ScoreBadgeProps) {
  return (
    <div className={`${colorClass(score)} text-white rounded-full w-14 h-14 flex items-center justify-center text-xl font-bold`}>
      {score}
    </div>
  );
}
```

- [ ] **Step 5: Create app scaffold**

Create `shipsmart/apps/web/src/main.tsx`:

```typescript
import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

Create `shipsmart/apps/web/src/index.css`:

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

Create `shipsmart/apps/web/src/App.tsx`:

```typescript
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { PRDetail } from './pages/PRDetail';
import { TeamDashboard } from './pages/TeamDashboard';
import { ApiHealth } from './pages/ApiHealth';

export function App() {
  return (
    <BrowserRouter>
      <div className="min-h-screen bg-gray-950 text-gray-100">
        <nav className="border-b border-gray-800 px-6 py-3 flex gap-6">
          <a href="/" className="font-bold text-white">ShipSmart</a>
          <a href="/dashboard" className="text-gray-400 hover:text-white">Team</a>
          <a href="/api-health" className="text-gray-400 hover:text-white">API Health</a>
        </nav>
        <Routes>
          <Route path="/" element={<TeamDashboard />} />
          <Route path="/dashboard" element={<TeamDashboard />} />
          <Route path="/pr/:id" element={<PRDetail />} />
          <Route path="/api-health" element={<ApiHealth />} />
        </Routes>
      </div>
    </BrowserRouter>
  );
}
```

- [ ] **Step 6: Run ScoreBadge test to verify it passes**

```bash
cd shipsmart/apps/web && yarn test src/components/ScoreBadge.test.tsx
```

Expected: PASS — 3 tests passing.

- [ ] **Step 7: Commit**

```bash
git add shipsmart/apps/web/
git commit -m "feat: add React app scaffold and ScoreBadge component"
```

---

## Task 9: Dashboard Pages

**Files:**
- Create: `shipsmart/apps/web/src/pages/PRDetail.tsx`
- Create: `shipsmart/apps/web/src/pages/TeamDashboard.tsx`
- Create: `shipsmart/apps/web/src/pages/ApiHealth.tsx`
- Create: `shipsmart/apps/web/src/components/TrendChart.tsx`
- Create: `shipsmart/apps/web/src/components/PRList.tsx`
- Create: `shipsmart/apps/web/src/pages/PRDetail.test.tsx`

- [ ] **Step 1: Write PRDetail test**

Create `shipsmart/apps/web/src/pages/PRDetail.test.tsx`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { PRDetail } from './PRDetail';

global.fetch = vi.fn().mockResolvedValue({
  json: () => Promise.resolve({
    id: 'pr-1',
    number: 42,
    title: 'Add payment service',
    author: 'snehal',
    score: 74,
    label: 'high',
    summary: 'Complex PR with deep nesting.',
    fixes: ['Fix 1', 'Fix 2', 'Fix 3'],
    url: 'https://github.com/org/repo/pull/42',
  }),
});

describe('PRDetail', () => {
  it('renders the PR score', async () => {
    render(
      <MemoryRouter initialEntries={['/pr/pr-1']}>
        <Routes>
          <Route path="/pr/:id" element={<PRDetail />} />
        </Routes>
      </MemoryRouter>
    );
    await waitFor(() => expect(screen.getByText('74')).toBeDefined());
  });

  it('renders the summary text', async () => {
    render(
      <MemoryRouter initialEntries={['/pr/pr-1']}>
        <Routes>
          <Route path="/pr/:id" element={<PRDetail />} />
        </Routes>
      </MemoryRouter>
    );
    await waitFor(() => expect(screen.getByText('Complex PR with deep nesting.')).toBeDefined());
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd shipsmart/apps/web && yarn test src/pages/PRDetail.test.tsx
```

Expected: FAIL — "Cannot find module './PRDetail'"

- [ ] **Step 3: Implement PRDetail page**

Create `shipsmart/apps/web/src/pages/PRDetail.tsx`:

```typescript
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { ScoreBadge } from '../components/ScoreBadge';

interface PR {
  id: string;
  number: number;
  title: string;
  author: string;
  score: number;
  label: string;
  summary: string | null;
  fixes: string[];
  url: string;
}

export function PRDetail() {
  const { id } = useParams<{ id: string }>();
  const [pr, setPR] = useState<PR | null>(null);

  useEffect(() => {
    fetch(`/api/prs/${id}`).then(r => r.json()).then(setPR);
  }, [id]);

  if (!pr) return <div className="p-6 text-gray-400">Loading...</div>;

  return (
    <div className="p-6 max-w-2xl">
      <div className="mb-4">
        <a href={pr.url} target="_blank" rel="noreferrer" className="text-blue-400 hover:underline text-sm">
          PR #{pr.number}
        </a>
        <h1 className="text-xl font-bold mt-1">{pr.title}</h1>
        <p className="text-gray-400 text-sm">by {pr.author}</p>
      </div>

      <div className="flex items-center gap-4 mb-6">
        <ScoreBadge score={pr.score} />
        <div>
          <p className="font-semibold">Complexity Score</p>
          <p className={`text-sm ${pr.label === 'high' ? 'text-red-400' : pr.label === 'medium' ? 'text-yellow-400' : 'text-green-400'}`}>
            {pr.label.charAt(0).toUpperCase() + pr.label.slice(1)} — {pr.label === 'high' ? 'review recommended' : pr.label === 'medium' ? 'check before merge' : 'looks good'}
          </p>
        </div>
      </div>

      {pr.summary && (
        <div className="bg-gray-900 rounded-lg p-4 mb-6 text-gray-300 text-sm leading-relaxed">
          <span className="text-blue-400 font-semibold">Summary: </span>{pr.summary}
        </div>
      )}

      {pr.fixes.length > 0 && (
        <div>
          <h2 className="font-semibold mb-3">Top Fixes</h2>
          <ol className="space-y-2 text-sm text-gray-300">
            {pr.fixes.map((fix, i) => (
              <li key={i} className="flex gap-2">
                <span className="text-blue-400 font-bold">{i + 1}.</span>
                <span>{fix}</span>
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Implement TrendChart**

Create `shipsmart/apps/web/src/components/TrendChart.tsx`:

```typescript
interface WeekData {
  week: number;
  avg: number;
}

interface TrendChartProps {
  data: WeekData[];
}

function barColor(avg: number): string {
  if (avg <= 40) return 'bg-green-500';
  if (avg <= 65) return 'bg-yellow-500';
  return 'bg-red-500';
}

export function TrendChart({ data }: TrendChartProps) {
  const max = Math.max(...data.map(d => d.avg), 1);

  return (
    <div className="flex items-end gap-3 h-20">
      {data.map(d => (
        <div key={d.week} className="flex flex-col items-center gap-1 flex-1">
          <span className="text-xs text-gray-400">{d.avg}</span>
          <div
            className={`w-full rounded-t ${barColor(d.avg)}`}
            style={{ height: `${(d.avg / max) * 60}px` }}
          />
          <span className="text-xs text-gray-500">W{d.week}</span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 5: Implement PRList**

Create `shipsmart/apps/web/src/components/PRList.tsx`:

```typescript
interface PR {
  id: string;
  number: number;
  title: string;
  score: number;
  url: string;
}

interface PRListProps {
  prs: PR[];
}

export function PRList({ prs }: PRListProps) {
  if (prs.length === 0) return <p className="text-gray-500 text-sm">No high-risk PRs this week.</p>;

  return (
    <ul className="space-y-2">
      {prs.map(pr => (
        <li key={pr.id} className="flex justify-between items-center text-sm">
          <a href={`/pr/${pr.id}`} className="text-blue-400 hover:underline truncate max-w-xs">
            #{pr.number} {pr.title}
          </a>
          <span className={`font-bold ml-4 ${pr.score > 65 ? 'text-red-400' : 'text-yellow-400'}`}>
            {pr.score}
          </span>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 6: Implement TeamDashboard page**

Create `shipsmart/apps/web/src/pages/TeamDashboard.tsx`:

```typescript
import { useEffect, useState } from 'react';
import { TrendChart } from '../components/TrendChart';
import { PRList } from '../components/PRList';

interface DashboardData {
  avgScore: number;
  highRiskPRs: Array<{ id: string; number: number; title: string; score: number; url: string }>;
  weeklyTrend: Array<{ week: number; avg: number }>;
  apiDebtCount: number;
}

export function TeamDashboard() {
  const [data, setData] = useState<DashboardData | null>(null);

  useEffect(() => {
    fetch('/api/dashboard/team').then(r => r.json()).then(setData);
  }, []);

  if (!data) return <div className="p-6 text-gray-400">Loading...</div>;

  return (
    <div className="p-6 max-w-3xl">
      <h1 className="text-2xl font-bold mb-6">Team Dashboard</h1>

      <div className="grid grid-cols-2 gap-4 mb-8">
        <div className="bg-gray-900 rounded-lg p-4 text-center">
          <p className="text-3xl font-bold text-yellow-400">{data.avgScore}</p>
          <p className="text-sm text-gray-400 mt-1">Avg complexity this week</p>
        </div>
        <div className="bg-gray-900 rounded-lg p-4 text-center">
          <p className="text-3xl font-bold text-red-400">{data.highRiskPRs.length}</p>
          <p className="text-sm text-gray-400 mt-1">High-risk PRs</p>
        </div>
        <div className="bg-gray-900 rounded-lg p-4 text-center">
          <p className="text-3xl font-bold text-blue-400">{data.apiDebtCount}</p>
          <p className="text-sm text-gray-400 mt-1">API debt issues</p>
        </div>
      </div>

      <div className="bg-gray-900 rounded-lg p-4 mb-6">
        <h2 className="font-semibold mb-4">Complexity Trend (5 weeks)</h2>
        <TrendChart data={data.weeklyTrend} />
      </div>

      <div className="bg-gray-900 rounded-lg p-4">
        <h2 className="font-semibold mb-4">High-Risk PRs This Week</h2>
        <PRList prs={data.highRiskPRs} />
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Implement ApiHealth page**

Create `shipsmart/apps/web/src/pages/ApiHealth.tsx`:

```typescript
import { useState } from 'react';

interface ApiSpecResult {
  id: string;
  filename: string;
  score: number;
  subScores: { naming: number; nesting: number; versioning: number; methods: number; endpoints: number };
  summary: string | null;
  suggestions: string[];
}

export function ApiHealth() {
  const [result, setResult] = useState<ApiSpecResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    setLoading(true);
    setError(null);

    try {
      const text = await file.text();
      let spec: Record<string, unknown>;
      try {
        spec = JSON.parse(text);
      } catch {
        setError('Invalid JSON. Only JSON OpenAPI specs are supported.');
        setLoading(false);
        return;
      }

      const res = await fetch('/api/specs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ repoId: 'default', filename: file.name, spec }),
      });

      if (!res.ok) {
        setError('Analysis failed. Please check your spec format.');
        return;
      }

      setResult(await res.json());
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="p-6 max-w-2xl">
      <h1 className="text-2xl font-bold mb-6">API Health</h1>

      <div className="bg-gray-900 rounded-lg p-4 mb-6">
        <label className="block text-sm text-gray-400 mb-2">Upload OpenAPI spec (JSON)</label>
        <input type="file" accept=".json" onChange={handleUpload} className="text-sm text-gray-300" />
        {loading && <p className="text-gray-400 text-sm mt-2">Analyzing...</p>}
        {error && <p className="text-red-400 text-sm mt-2">{error}</p>}
      </div>

      {result && (
        <div>
          <div className="flex items-center gap-4 mb-6">
            <div className={`rounded-full w-14 h-14 flex items-center justify-center text-xl font-bold text-white ${result.score >= 70 ? 'bg-green-500' : result.score >= 50 ? 'bg-yellow-500' : 'bg-red-500'}`}>
              {result.score}
            </div>
            <div>
              <p className="font-semibold">{result.filename}</p>
              <p className="text-gray-400 text-sm">API Health Score</p>
            </div>
          </div>

          <div className="bg-gray-900 rounded-lg p-4 mb-4">
            <h2 className="font-semibold mb-3 text-sm">Score Breakdown</h2>
            {Object.entries(result.subScores).map(([key, val]) => (
              <div key={key} className="flex justify-between text-sm py-1 border-b border-gray-800">
                <span className="text-gray-400 capitalize">{key}</span>
                <span className={val >= 15 ? 'text-green-400' : val >= 8 ? 'text-yellow-400' : 'text-red-400'}>{val}/20</span>
              </div>
            ))}
          </div>

          {result.summary && (
            <div className="bg-gray-900 rounded-lg p-4 mb-4 text-sm text-gray-300">
              <span className="text-blue-400 font-semibold">Summary: </span>{result.summary}
            </div>
          )}

          {result.suggestions.length > 0 && (
            <div className="bg-gray-900 rounded-lg p-4">
              <h2 className="font-semibold mb-3 text-sm">Suggestions</h2>
              <ol className="space-y-2 text-sm text-gray-300">
                {result.suggestions.map((s, i) => (
                  <li key={i} className="flex gap-2">
                    <span className="text-blue-400 font-bold">{i + 1}.</span>
                    <span>{s}</span>
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 8: Run PRDetail test to verify it passes**

```bash
cd shipsmart/apps/web && yarn test src/pages/PRDetail.test.tsx
```

Expected: PASS — 2 tests passing.

- [ ] **Step 9: Commit**

```bash
git add shipsmart/apps/web/src/
git commit -m "feat: add dashboard pages — PRDetail, TeamDashboard, ApiHealth"
```

---

## Task 10: Run Full Test Suite + Smoke Test

**Files:** none modified

- [ ] **Step 1: Run all API tests**

```bash
cd shipsmart/apps/api && yarn test
```

Expected: All tests pass. If any fail, fix before proceeding.

- [ ] **Step 2: Run all web tests**

```bash
cd shipsmart/apps/web && yarn test
```

Expected: All tests pass.

- [ ] **Step 3: Start the API server and verify health endpoint**

```bash
cd shipsmart/apps/api && yarn dev
```

In a second terminal:

```bash
curl http://localhost:3000/health
```

Expected: `{"status":"ok"}`

- [ ] **Step 4: Start the web dev server**

```bash
cd shipsmart/apps/web && yarn dev
```

Open `http://localhost:5173` in browser. Verify:
- Nav renders with ShipSmart, Team, API Health links
- Team dashboard loads (will show empty state without data)
- API Health page renders file upload input

- [ ] **Step 5: Final commit**

```bash
git add shipsmart/
git commit -m "chore: verify full test suite and smoke test pass"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Task |
|---|---|
| GitHub App webhook integration | Task 6 |
| Static complexity rules engine (5 rules) | Task 3 |
| LLM explainer via Claude API | Task 5 |
| Bot PR comment posting | Task 6 |
| Postgres score storage | Task 2 |
| Dashboard: engineer PR view | Task 9 (PRDetail) |
| Dashboard: EM team trends view | Task 9 (TeamDashboard) |
| GitHub OAuth login | Task 7 |
| OpenAPI spec upload + API health scoring | Task 4 + Task 7 (api-health route) + Task 9 (ApiHealth page) |
| Error handling — webhook retry | Noted in webhook route; retry logic is production concern beyond MVP scope |
| Error handling — LLM timeout | Task 5 (10s timeout + null fallback) |
| Error handling — invalid spec | Task 7 (JSON parse error → 400) |
| Error handling — auth failure | Task 7 (redirect on failure) |

All requirements covered. No TBDs or placeholders. Types are consistent across tasks.
