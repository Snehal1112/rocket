import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const src = (path: string) => readFileSync(resolve(process.cwd(), `src/${path}`), 'utf8');

describe('sunken surfaces', () => {
  it.each([
    ['components/settings/SettingsPage.tsx'],
    ['components/settings/sections/AiAgentsSection.tsx'],
    ['components/layout/ConsolePanel.tsx'],
  ])('%s uses the sunken token', (path) => {
    expect(src(path)).toContain('bg-elevation-sunken');
  });

  it('console output well is no longer a translucent background', () => {
    expect(src('components/layout/ConsolePanel.tsx')).not.toContain('bg-background/60');
  });

  it('settings no longer names the editor token directly', () => {
    expect(src('components/settings/SettingsPage.tsx')).not.toContain('bg-editor-bg');
    expect(src('components/settings/sections/AiAgentsSection.tsx')).not.toContain('bg-editor-bg');
  });
});

describe('chrome layer', () => {
  it('breadcrumb row casts the chrome shadow above the content', () => {
    const file = src('components/panes/BreadcrumbBar.tsx');
    expect(file).toContain('elevation-chrome');
    expect(file).toContain('relative z-[1]');
  });

  it('collection overview header uses the shared chrome shadow when scrolled', () => {
    const file = src('components/collections/CollectionOverviewTab.tsx');
    expect(file).toContain("isOverviewScrolled && 'elevation-chrome'");
    expect(file).not.toContain('shadow-[0_2px_8px_rgba(0,0,0,0.08)]');
  });

  it('upper chrome rows stay plain so shadows do not stack', () => {
    for (const path of [
      'components/panes/TabBar.tsx',
      'components/layout/WorkspaceToolbar.tsx',
      'components/request/RocketTabBar.tsx',
    ]) {
      expect(src(path)).not.toContain('elevation-chrome');
    }
  });

  it.each([
    ['components/panes/BreadcrumbBar.tsx'],
    ['components/collections/CollectionOverviewTab.tsx'],
    ['components/panes/TabBar.tsx'],
    ['components/panes/TabItem.tsx'],
    ['components/layout/WorkspaceToolbar.tsx'],
    ['components/request/RocketTabBar.tsx'],
    ['components/request/RequestPanel.tsx'],
  ])('%s adds no border opacity variants', (path) => {
    expect(src(path)).not.toMatch(/border-border\//);
  });
});

describe('sunken wells and overview cards', () => {
  const panel = () => src('components/request/RequestPanel.tsx');

  it('request and response wells use the sunken level, not bg-card', () => {
    const file = panel();
    expect(file).toContain("'flex flex-col overflow-hidden bg-elevation-sunken'");
    expect(file).toContain('flex-1 flex flex-col overflow-hidden bg-elevation-sunken min-w-0');
    expect(file).toContain('flex-1 flex flex-col overflow-hidden bg-elevation-sunken min-h-0');
    expect(file).toContain('flex-1 flex flex-col min-h-0 bg-elevation-sunken');
    expect(file).not.toMatch(/bg-card/);
  });

  it('URL row stays on the chrome (default surface) level', () => {
    expect(panel()).toContain('px-3 py-2 bg-elevation-surface');
  });

  it('both splitters sit on the default surface between the wells', () => {
    const file = panel();
    expect(file).not.toContain('bg-muted/30 border-border');
    expect(file.match(/'bg-elevation-surface border-border hover:bg-accent\/50/g)).toHaveLength(2);
  });

  it('section tab row and response header rows are chrome level', () => {
    expect(src('components/request/RocketTabBar.tsx')).toContain(
      'border-b border-border bg-elevation-surface px-3',
    );
    const viewer = src('components/response/ResponseBodyViewer.tsx');
    expect(viewer).toContain('px-3 py-1.5 shrink-0 bg-elevation-surface');
    expect(viewer).toContain('px-1 shrink-0 bg-elevation-surface');
    expect(viewer).not.toContain('bg-card');
  });

  it('overview pages are sunken so cards stand out above them', () => {
    expect(src('components/collections/CollectionOverviewTab.tsx')).toContain(
      'relative flex-1 min-h-0 h-full bg-elevation-sunken',
    );
    expect(src('components/workspace/WorkspaceOverviewTab.tsx')).toContain(
      'flex h-full overflow-hidden bg-elevation-sunken',
    );
  });

  it.each([
    ['components/request/RequestPanel.tsx', 0],
    ['components/request/RocketTabBar.tsx', 0],
    ['components/response/ResponseBodyViewer.tsx', 0],
    ['components/collections/CollectionOverviewTab.tsx', 0],
    // One existing dashed empty-state border, which may fade.
    ['components/workspace/WorkspaceOverviewTab.tsx', 1],
  ])('%s adds no solid border opacity variants', (path, allowed) => {
    expect(src(path).match(/border-border\/\d/g)?.length ?? 0).toBe(allowed);
  });
});

describe('lift, overlay shadow and pinned edges', () => {
  it('breadcrumb row sits on the default surface like the rows around it', () => {
    expect(src('components/panes/BreadcrumbBar.tsx')).toContain('elevation-surface elevation-chrome relative z-[1]');
  });

  it('overview scroll listeners re-attach once the page has rendered', () => {
    const text = src('components/collections/CollectionOverviewTab.tsx');
    expect(text).toContain('}, [loading, activeSection]);');
    expect(text.match(/\}, \[loading, activeSection\]\);/g)?.length).toBe(2);
  });

  it('console header casts the chrome shadow over the log list', () => {
    const src = readFileSync(resolve(process.cwd(), 'src/components/layout/ConsolePanel.tsx'), 'utf8');
    expect(src).toContain(
      "'elevation-chrome relative z-[1] flex items-center gap-2 px-2 py-1 border-b border-border shrink-0'",
    );
  });

  const floating = [
    'components/ui/popover.tsx',
    'components/ui/dropdown-menu.tsx',
    'components/ui/context-menu.tsx',
    'components/ui/select.tsx',
    'components/ui/dialog.tsx',
    'components/ui/alert-dialog.tsx',
    'components/ui/sheet.tsx',
    'components/editor/extensions/variable-hover.ts',
    'components/editor/VariablePopover.tsx',
  ];

  it('primary button lifts, other variants keep shadow-xs', () => {
    const button = src('components/ui/button.tsx');
    const body = (name: string) => {
      const lines = button.split('\n');
      return lines[lines.findIndex((l) => l.includes(`${name}:`)) + 1];
    };
    expect(body('default')).toContain(
      'shadow-(--elevation-shadow-lift) active:shadow-none disabled:shadow-none',
    );
    expect(body('default')).not.toContain('elevation-lift');
    expect(body('default')).not.toContain('shadow-xs');
    expect(body('destructive')).toContain('shadow-xs');
    expect(body('outline')).toContain('shadow-xs');
    expect(body('secondary')).toContain('shadow-xs');
  });

  it.each(floating)('%s uses the overlay shadow only', (path) => {
    const text = src(path);
    expect(text).toContain('shadow-(--elevation-shadow-overlay)');
    expect(text).not.toMatch(/shadow-(md|lg)\b/);
  });

  // The fills the floating surfaces had before the elevation work. Shadow-only changes must not touch them.
  const fills: Record<string, string> = {
    'components/ui/popover.tsx': 'bg-dropdown-bg/50',
    'components/ui/dropdown-menu.tsx': 'bg-accent bg-border/60 bg-dropdown-bg/50',
    'components/ui/context-menu.tsx': 'bg-accent bg-border/60 bg-dropdown-bg/50',
    'components/ui/select.tsx': 'bg-accent bg-dropdown-bg bg-dropdown-bg/50 bg-muted dark:bg-input/30',
    'components/ui/dialog.tsx': 'bg-accent bg-black/80 bg-card',
    'components/ui/alert-dialog.tsx': 'bg-black/80 bg-card',
    'components/ui/sheet.tsx': 'bg-background bg-background/20',
    'components/editor/extensions/variable-hover.ts': 'bg-card',
    'components/editor/VariablePopover.tsx': 'bg-card/50 bg-muted/30 bg-transparent dark:bg-transparent',
  };

  it.each(floating)('%s keeps its fills', (path) => {
    const used = [...new Set(src(path).match(/(?:dark:)?bg-[A-Za-z0-9/.-]*/g) ?? [])].sort();
    expect(used.join(' ')).toBe(fills[path].split(' ').sort().join(' '));
  });

  it('status bar is a pinned edge above the side panels (z-5)', () => {
    expect(src('components/layout/StatusBar.tsx')).toContain('elevation-pinned relative z-[6]');
  });
});
