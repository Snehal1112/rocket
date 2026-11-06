# Rocket user manual

The user manual for **Rocket**, the native desktop API client — task-oriented
guides for every feature, separate from `docs/` in the main repo, which is
internal planning and engineering material, not end-user documentation.

This is a standalone static site, built the same way `rocket-landing` is —
same stack, same brand tokens, same shared components — but it does not import
from either at runtime. It's a separate Vite project with its own deploy;
components and tokens are copied in, not imported live.

## Run it

```bash
bun install   # once
bun run dev   # http://localhost:5173, with hot reload
bun run build # tsc -b && vite build, output in dist/
bun run lint  # oxlint
```

`bun run build` is the gate: it must finish with no TypeScript errors, and
`bun run lint` must finish with no warnings.

## Stack

Vite · React 19 · TypeScript · Tailwind CSS v4 · shadcn/ui · lucide-react ·
`react-router-dom` for client routing · `react-markdown` + `remark-gfm` +
`rehype-slug` to render the guide content — the same toolchain as
`rocket-landing`, plus routing and markdown rendering, which a one-page
marketing site doesn't need but a multi-page manual does.

## Where things are

| Path | What it holds |
|---|---|
| `src/content/guide/` | One markdown file per topic — the actual manual content |
| `src/lib/content.ts` | Maps each page's slug to its markdown source (`?raw` import) |
| `src/lib/sidebar.ts` | The sidebar's groups/order, and the outbound links (GitHub, the main site) |
| `src/pages/Home.tsx` | The manual's home page (hero + feature grid) |
| `src/pages/GuidePage.tsx` | Renders one guide page: looks up its markdown, feeds it to `MarkdownPage`, builds the on-page outline |
| `src/components/MarkdownPage.tsx` | The markdown renderer — wires `react-markdown`, the image lightbox, and internal link handling |
| `src/components/ImageLightbox.tsx`, `Reveal.tsx`, `ui/*` | Copied from `rocket-landing` verbatim — not reimplemented, not imported cross-project |
| `src/index.css` | Same color tokens and fonts as `rocket-landing/src/index.css`, plus hand-rolled markdown ("prose") styling since there's no typography plugin in this stack |
| `public/screenshots/` | Real app screenshots |

## Keeping brand tokens in sync

`src/index.css`'s color tokens and fonts are copied from
`rocket-landing/src/index.css`, not imported — they're separate Vite projects.
If the brand palette ever changes, update both files by hand.

## Adding a new page

1. Add the markdown file under `src/content/guide/`.
2. Import it in `src/lib/content.ts` (as `?raw`) and add it to the `content` map.
3. Add it to the right group in `src/lib/sidebar.ts`.
4. Run `bun run build` before committing.

## Deploying

See `.claude/deploy-railway.md` — not deployed yet; that file has the
one-time setup plus the steps for every deploy after.
