# Theme Palette Update — Design Spec

**Date:** 2026-04-26
**File affected:** `src/globals.css` (CSS custom properties only)

---

## Goal

Update both dark and light theme CSS tokens in `globals.css`:
- **Dark:** Match the color palette visible in the reference screenshot (Rocket app, dark mode)
- **Light:** Mathematically derived from dark ratios + three Option B corrections (muted-foreground, accent/border separation, radius)

---

## Dark Theme — New Values

| Variable | Old value | New value | Hex equiv |
|---|---|---|---|
| `--background` | `0 0% 9.4%` | `0 0% 11%` | `#1C1C1C` |
| `--foreground` | `0 0% 80%` | `0 0% 83%` | `#D4D4D4` |
| `--card` | `0 0% 12%` | `0 0% 14.5%` | `#252526` |
| `--card-foreground` | `0 0% 80%` | `0 0% 83%` | `#D4D4D4` |
| `--popover` | `0 0% 12%` | `0 0% 14.5%` | `#252526` |
| `--popover-foreground` | `0 0% 80%` | `0 0% 83%` | `#D4D4D4` |
| `--primary` | `206 100% 41.6%` | `206 100% 41.6%` | `#0078D4` (unchanged) |
| `--primary-foreground` | `0 0% 100%` | `0 0% 100%` | unchanged |
| `--secondary` | `0 0% 17%` | `0 0% 17.6%` | `#2D2D2D` |
| `--secondary-foreground` | `0 0% 80%` | `0 0% 83%` | `#D4D4D4` |
| `--muted` | `0 0% 19.2%` | `0 0% 17.6%` | `#2D2D2D` |
| `--muted-foreground` | `0 0% 62%` | `0 0% 61.6%` | `#9D9D9D` |
| `--accent` | `0 0% 17%` | `0 0% 16.5%` | `#2A2A2A` |
| `--accent-foreground` | `0 0% 80%` | `0 0% 83%` | `#D4D4D4` |
| `--border` | `0 0% 17%` | `0 0% 23.5%` | `#3C3C3C` |
| `--input` | `0 0% 19.2%` | `0 0% 23.5%` | `#3C3C3C` |
| `--ring` | `206 100% 41.6%` | `206 100% 41.6%` | unchanged |
| `--destructive` | `0 72% 45%` | `0 71% 61.6%` | `#F14C4C` |
| `--warning` | `38 92% 50%` | `45 100% 40%` | `#CCA700` |
| `--radius` | `0.7rem` | `0.3rem` | shared with light |

Chart colors — unchanged.

---

## Light Theme — New Values

| Variable | Old value | New value | Hex equiv |
|---|---|---|---|
| `--background` | `0 0% 97.3%` | `0 0% 96.9%` | `#F7F7F7` |
| `--foreground` | `0 0% 23%` | `0 0% 18%` | `#2E2E2E` |
| `--card` | `0 0% 100%` | `0 0% 100%` | `#FFFFFF` (unchanged) |
| `--card-foreground` | `0 0% 23%` | `0 0% 18%` | `#2E2E2E` |
| `--popover` | `0 0% 100%` | `0 0% 100%` | unchanged |
| `--popover-foreground` | `0 0% 23%` | `0 0% 18%` | `#2E2E2E` |
| `--primary` | `209 100% 36.1%` | `209 100% 36.1%` | `#005FB8` (unchanged) |
| `--primary-foreground` | `0 0% 100%` | `0 0% 100%` | unchanged |
| `--secondary` | `0 0% 90%` | `0 0% 91%` | `#E8E8E8` |
| `--secondary-foreground` | `0 0% 23%` | `0 0% 18%` | `#2E2E2E` |
| `--muted` | `0 0% 95%` | `0 0% 93.5%` | `#EEEEEE` |
| `--muted-foreground` | `0 0% 53%` | `0 0% 44%` | `#707070` ← **Option B fix** |
| `--accent` | `0 0% 91%` | `0 0% 92%` | `#EBEBEB` |
| `--accent-foreground` | `0 0% 23%` | `0 0% 18%` | `#2E2E2E` |
| `--border` | `0 0% 90%` | `0 0% 85%` | `#D9D9D9` ← more visible |
| `--input` | `0 0% 80.8%` | `0 0% 80%` | `#CCCCCC` |
| `--ring` | `209 100% 36.1%` | `209 100% 36.1%` | unchanged |
| `--destructive` | `0 84.2% 60.2%` | `0 71% 50%` | `#E03B3B` |
| `--warning` | `38 92% 50%` | `45 90% 42%` | `#C9A000` |
| `--radius` | `0.7rem` | `0.3rem` | shared with dark ← **Option B fix** |

Chart colors — unchanged.

---

## Derivation Method

Dark palette extracted pixel-precisely from reference screenshot + cross-referenced with VS Code Dark Modern source.

Light palette derived by preserving dark contrast ratios:
- Background elevation delta: dark `+3.5%` L → light `-3%` L (card above bg)
- Border delta: dark `+12.5%` above bg → light `-12%` below bg
- Muted delta: dark `17.6%` → light `93.5%`
- Accent kept 1% separated from border on both modes

Three Option B corrections applied to light only:
1. `--muted-foreground`: `53%` → `44%` (secondary text was too washed-out)
2. `--accent` vs `--border`: now distinct (hover states visible on bordered surfaces)
3. `--radius`: `0.7rem` → `0.3rem` (matches VS Code's 4–6px border-radius feel)

---

## Non-Goals

- No changes to `tailwind.config.js`
- No changes to chart colors
- No changes to component markup
- No changes to font imports or scrollbar CSS
- No changes to Linux window chrome styles
- No changes to nested card elevation rules (those reference `--border` and `--radius` variables and will update automatically)
