# Deploying rocket-docs to Railway

The manual is a Vite + React static site — same stack as `rocket-landing`,
including the same Dockerfile/Caddyfile pattern — served by Caddy from a
Docker image, as a separate sibling Railway service.

## Status

Not deployed yet. This is a one-time setup: create the Railway service once,
then every later deploy is the same `railway up` command `rocket-landing`
already uses.

## Files

- `Dockerfile`: a Node 22 plus bun build stage (`bun run build` runs
  `tsc -b && vite build`, output to `dist/`), then a `caddy:2-alpine` runtime
  stage — identical shape to `rocket-landing`'s Dockerfile.
- `Caddyfile`: listens on `$PORT`, caches `/assets/*` for a year (Vite hashes
  those filenames), serves `/screenshots/*`, `/favicon.png` and `/rocket.png`
  with a short cache, and falls back every other route to `/index.html` —
  this is a client-routed SPA (`react-router-dom`), same pattern as
  `rocket-landing`, not VitePress's old per-page static output.
- `railway.json`: Dockerfile builder, health check on `/`.
- `.dockerignore` / `.gitignore`: keep `node_modules` and `dist` out of the
  build context and git.

## First deploy (one-time)

```bash
cd rocket-docs
railway link -p 29691ce6-0cf1-4db6-9323-7ea3e14e66d2 -e production
railway add --service rocket-docs
railway up --service rocket-docs --environment production --ci
```

Uses the same Railway project as `rocket-landing` (`rocket-app`,
`29691ce6-0cf1-4db6-9323-7ea3e14e66d2`), as a new sibling service. After the
first deploy, add a custom domain in the Railway dashboard for the
`rocket-docs` service — `docs.rocket.numericlabs.io` would match the existing
`web.rocket.numericlabs.io` pattern used for the landing page.

## Later deploys

```bash
cd rocket-docs
railway up --service rocket-docs --environment production --ci
```

Deploys are manual, same as `rocket-landing` — a git push does not deploy.

## Landing page link

`links.docs` in `rocket-landing/src/lib/site.ts` already points at
`https://docs.rocket.numericlabs.io` — the nav and footer both read from that
one constant. That URL will 404 until this service is deployed and the custom
domain above is attached; if a different subdomain gets used instead, update
that one constant to match.
