import {
  BookOpen,
  ChevronRight,
  FileLock,
  FolderOpen,
  GitBranch,
  Globe,
  KeyRound,
  LayoutDashboard,
  List,
  Loader2,
  Variable,
} from 'lucide-react';
import type React from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import { mapApiRequestToState } from '@/lib/pane-utils';
import { getCollection, listCollections } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionSection, Tab, WorkspaceTabSection } from '@/types/pane-types';
import {
  isCollectionTab,
  isConflictTab,
  isContractTab,
  isDiffTab,
  isGitTab,
  isRequestTab,
  isWorkspaceTab,
} from '@/types/pane-types';

// ── Picker types ────────────────────────────────────────────────────

interface PickerItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
  isActive?: boolean;
}

interface Picker {
  loadItems: () => PickerItem[] | Promise<PickerItem[]>;
  onSelect: (item: PickerItem) => void;
}

// ── Segment ─────────────────────────────────────────────────────────

interface Segment {
  label: string;
  icon?: React.ReactNode;
  picker?: Picker;
}

// ── Helpers ─────────────────────────────────────────────────────────

function collectionBasename(absPath: string): string {
  return absPath.split('/').filter(Boolean).pop() ?? absPath;
}

function workspaceSectionLabel(section: WorkspaceTabSection): string {
  switch (section) {
    case 'overview':
      return 'Overview';
    case 'environments':
      return 'Environments';
    case 'git':
      return 'Git';
    case 'audit':
      return 'Audit';
  }
}

function collectionSectionLabel(section: CollectionSection | undefined): string {
  switch (section) {
    case 'auth':
      return 'Authorization';
    case 'variables':
      return 'Variables';
    case 'documentation':
      return 'Documentation';
    default:
      return 'Overview';
  }
}

function collectionSectionIcon(section: CollectionSection): React.ReactNode {
  switch (section) {
    case 'auth':
      return <KeyRound className='h-3 w-3' />;
    case 'variables':
      return <Variable className='h-3 w-3' />;
    case 'documentation':
      return <BookOpen className='h-3 w-3' />;
    default:
      return <List className='h-3 w-3' />;
  }
}

function workspaceSectionIcon(section: WorkspaceTabSection): React.ReactNode {
  switch (section) {
    case 'overview':
      return <LayoutDashboard className='h-3 w-3' />;
    case 'environments':
      return <Globe className='h-3 w-3' />;
    case 'git':
      return <GitBranch className='h-3 w-3' />;
    case 'audit':
      return <List className='h-3 w-3' />;
  }
}

// ── Segment derivation ──────────────────────────────────────────────

interface NavActions {
  openCollectionTab: (collection: string, section: CollectionSection) => void;
  openWorkspaceTabs: (workspaceId: string, section: WorkspaceTabSection) => void;
  openTab: (tab: Tab) => void;
  updateCollectionSection: (tabId: string, section: CollectionSection) => void;
}

function deriveSegments(
  tab: Tab,
  workspaceName: string,
  workspaces: Array<{ id: string; name: string }>,
  nav: NavActions,
): Segment[] {
  if (isRequestTab(tab)) {
    if (!tab.source) return [{ label: 'Unsaved Request' }];

    const { collection, path } = tab.source;
    const parts = path.split('/').filter(Boolean);

    const collectionSegment: Segment = {
      label: collection,
      picker: {
        loadItems: async () => {
          const summaries = await listCollections();
          return summaries.map((s) => ({
            id: s.name,
            label: s.name,
            isActive: s.name === collection,
          }));
        },
        onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
      },
    };

    const segments: Segment[] = [collectionSegment];

    for (let i = 0; i < parts.length - 1; i++) {
      const folderName = parts[i];
      const parentPath = parts.slice(0, i).join('/');
      segments.push({
        label: folderName,
        icon: <FolderOpen className='h-3 w-3' />,
        picker: {
          loadItems: async () => {
            const col = await getCollection(collection);
            let items = col.root.items;
            const parentParts = parentPath ? parentPath.split('/') : [];
            for (const part of parentParts) {
              const found = items.find((it) => it.type === 'folder' && it.name === part);
              if (found && found.type === 'folder') items = found.items;
              else break;
            }
            return items.map((it) => ({
              id: it.type === 'folder' ? it.name : (it.fileName ?? it.name),
              label: it.name,
              icon:
                it.type === 'folder' ? (
                  <FolderOpen className='h-3 w-3' />
                ) : (
                  <span className={`font-semibold text-2xs ${METHOD_TEXT_COLOR[it.method] ?? ''}`}>
                    {it.method}
                  </span>
                ),
              isActive: it.name === folderName,
            }));
          },
          onSelect: async (item) => {
            const col = await getCollection(collection);
            let items = col.root.items;
            const parentParts = parentPath ? parentPath.split('/') : [];
            for (const part of parentParts) {
              const found = items.find((it) => it.type === 'folder' && it.name === part);
              if (found && found.type === 'folder') items = found.items;
              else break;
            }
            const sibling = items.find((it) =>
              it.type === 'folder' ? it.name === item.label : (it.fileName ?? it.name) === item.id,
            );
            if (!sibling || sibling.type !== 'request') return;
            nav.openTab({
              id: sibling.uid,
              title: sibling.name,
              tabType: 'request',
              request: mapApiRequestToState(sibling, true),
              response: null,
              isDirty: false,
              source: {
                collection,
                path: parentPath
                  ? `${parentPath}/${sibling.fileName ?? sibling.name}`
                  : (sibling.fileName ?? sibling.name),
              },
            });
          },
        },
      });
    }

    const requestName = parts[parts.length - 1] ?? path;
    const parentPath = parts.slice(0, parts.length - 1).join('/');
    const methodIcon = (
      <span className={`font-semibold text-2xs ${METHOD_TEXT_COLOR[tab.request.method] ?? ''}`}>
        {tab.request.method}
      </span>
    );
    segments.push({
      label: requestName,
      icon: methodIcon,
      picker: {
        loadItems: async () => {
          const col = await getCollection(collection);
          let items = col.root.items;
          const parentParts = parentPath ? parentPath.split('/') : [];
          for (const part of parentParts) {
            const found = items.find((it) => it.type === 'folder' && it.name === part);
            if (found && found.type === 'folder') items = found.items;
            else break;
          }
          return items
            .filter((it) => it.type === 'request')
            .map((it) => ({
              id: it.fileName ?? it.name,
              label: it.name,
              icon: (
                <span className={`font-semibold text-2xs ${METHOD_TEXT_COLOR[it.method] ?? ''}`}>
                  {it.method}
                </span>
              ),
              isActive: (it.fileName ?? it.name) === requestName,
            }));
        },
        onSelect: async (item) => {
          const col = await getCollection(collection);
          let items = col.root.items;
          const parentParts = parentPath ? parentPath.split('/') : [];
          for (const part of parentParts) {
            const found = items.find((it) => it.type === 'folder' && it.name === part);
            if (found && found.type === 'folder') items = found.items;
            else break;
          }
          const req = items.find(
            (it) => it.type === 'request' && (it.fileName ?? it.name) === item.id,
          );
          if (!req || req.type !== 'request') return;
          nav.openTab({
            id: req.uid,
            title: req.name,
            tabType: 'request',
            request: mapApiRequestToState(req, true),
            response: null,
            isDirty: false,
            source: {
              collection,
              path: parentPath ? `${parentPath}/${item.id}` : item.id,
            },
          });
        },
      },
    });

    return segments;
  }

  if (isCollectionTab(tab)) {
    const ALL_SECTIONS: CollectionSection[] = ['overview', 'auth', 'variables', 'documentation'];
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
        },
      },
      {
        label: collectionSectionLabel(tab.activeSection),
        picker: {
          loadItems: () =>
            ALL_SECTIONS.map((s) => ({
              id: s,
              label: collectionSectionLabel(s),
              icon: collectionSectionIcon(s),
              isActive: s === (tab.activeSection ?? 'overview'),
            })),
          onSelect: (item) => nav.updateCollectionSection(tab.id, item.id as CollectionSection),
        },
      },
    ];
  }

  if (isWorkspaceTab(tab)) {
    const ALL_WS_SECTIONS: WorkspaceTabSection[] = ['overview', 'environments', 'git', 'audit'];
    return [
      {
        label: workspaceName,
        icon: <LayoutDashboard className='h-3 w-3' />,
        picker: {
          loadItems: () =>
            workspaces.map((w) => ({
              id: w.id,
              label: w.name,
              icon: <LayoutDashboard className='h-3 w-3' />,
              isActive: w.id === tab.workspaceId,
            })),
          onSelect: (item) => nav.openWorkspaceTabs(item.id, 'overview'),
        },
      },
      {
        label: workspaceSectionLabel(tab.activeSection),
        picker: {
          loadItems: () =>
            ALL_WS_SECTIONS.map((s) => ({
              id: s,
              label: workspaceSectionLabel(s),
              icon: workspaceSectionIcon(s),
              isActive: s === tab.activeSection,
            })),
          onSelect: (item) =>
            nav.openWorkspaceTabs(tab.workspaceId, item.id as WorkspaceTabSection),
        },
      },
    ];
  }

  if (isGitTab(tab)) {
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
        },
      },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
    ];
  }

  if (isDiffTab(tab)) {
    const collection = collectionBasename(tab.diffState.collectionPath);
    return [
      {
        label: collection,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === collection,
            }));
          },
          onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
        },
      },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.diffState.filePath },
    ];
  }

  if (isConflictTab(tab)) {
    const collection = collectionBasename(tab.conflictState.collectionPath);
    return [
      {
        label: collection,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === collection,
            }));
          },
          onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
        },
      },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.conflictState.filePath },
    ];
  }

  if (isContractTab(tab)) {
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.openCollectionTab(item.id, 'overview'),
        },
      },
      { label: 'Contracts', icon: <FileLock className='h-3 w-3' /> },
    ];
  }

  const _exhaustive: never = tab;
  throw new Error(`Unhandled tab type: ${(_exhaustive as { tabType: string }).tabType}`);
}

// ── SegmentButton ───────────────────────────────────────────────────

interface SegmentButtonProps {
  seg: Segment;
  isLast: boolean;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}

function SegmentButton({ seg, isLast, isOpen, onOpenChange }: SegmentButtonProps) {
  const [items, setItems] = useState<PickerItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const fgClass = isLast ? 'text-breadcrumb-focus-fg' : 'text-breadcrumb-fg';

  const loadAndOpen = useCallback(async () => {
    if (!seg.picker) return;
    setLoading(true);
    try {
      const result = await seg.picker.loadItems();
      setItems(result);
      const activeIdx = result.findIndex((it) => it.isActive);
      setActiveIndex(activeIdx >= 0 ? activeIdx : 0);
    } finally {
      setLoading(false);
    }
  }, [seg.picker]);

  useEffect(() => {
    if (isOpen) void loadAndOpen();
  }, [isOpen, loadAndOpen]);

  useEffect(() => {
    if (!loading && isOpen && listRef.current) {
      const active = listRef.current.querySelector('[data-active="true"]');
      active?.scrollIntoView({ block: 'nearest' });
    }
  }, [loading, isOpen]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!isOpen) return;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, items.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const item = items[activeIndex];
        if (item) {
          seg.picker?.onSelect(item);
          onOpenChange(false);
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onOpenChange(false);
      }
    },
    [isOpen, items, activeIndex, seg.picker, onOpenChange],
  );

  const inner = (
    <>
      {seg.icon && <span className={fgClass}>{seg.icon}</span>}
      <span className={`text-xs ${fgClass}`}>{seg.label}</span>
    </>
  );

  if (!seg.picker) {
    return <span className='flex items-center gap-1 px-1'>{inner}</span>;
  }

  return (
    <Popover open={isOpen} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <Button
          type='button'
          variant='ghost'
          size='sm'
          onKeyDown={handleKeyDown}
          className={`h-auto py-0 px-1 rounded-sm font-normal hover:bg-accent/60 transition-colors ${fgClass} ${isOpen ? 'bg-accent/60' : ''}`}
        >
          {inner}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align='start'
        className='p-0 w-56 max-h-72 overflow-y-auto'
        onKeyDown={handleKeyDown}
      >
        {loading ? (
          <div className='flex items-center justify-center h-12'>
            <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
          </div>
        ) : items.length === 0 ? (
          <div className='px-3 py-2 text-xs text-muted-foreground'>No items</div>
        ) : (
          <div ref={listRef} role='listbox' aria-label='Pick item'>
            {items.map((item, idx) => (
              <Button
                key={item.id}
                type='button'
                variant='ghost'
                role='option'
                aria-selected={idx === activeIndex}
                data-active={item.isActive ? 'true' : undefined}
                className={`w-full justify-start gap-2 px-3 h-7 text-xs rounded-none transition-colors ${
                  idx === activeIndex
                    ? 'bg-accent text-accent-foreground'
                    : 'hover:bg-accent/50 text-foreground'
                } ${item.isActive ? 'font-medium' : ''}`}
                onClick={() => {
                  seg.picker?.onSelect(item);
                  onOpenChange(false);
                }}
                onMouseEnter={() => setActiveIndex(idx)}
              >
                {item.icon && <span className='shrink-0 text-muted-foreground'>{item.icon}</span>}
                <span className='truncate'>{item.label}</span>
              </Button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}

// ── BreadcrumbBar ───────────────────────────────────────────────────

interface BreadcrumbBarProps {
  tab: Tab;
}

export function BreadcrumbBar({ tab }: BreadcrumbBarProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspaceName = workspaces.find((w) => w.id === activeWorkspaceId)?.name ?? 'Workspace';

  const openCollectionTab = usePaneStore((s) => s.openCollectionTab);
  const openWorkspaceTabs = usePaneStore((s) => s.openWorkspaceTabs);
  const openTab = usePaneStore((s) => s.openTab);
  const updateCollectionSection = usePaneStore((s) => s.updateCollectionSection);

  const nav: NavActions = {
    openCollectionTab,
    openWorkspaceTabs,
    openTab,
    updateCollectionSection,
  };

  const segments = deriveSegments(tab, workspaceName, workspaces, nav);
  const [openPickerIndex, setOpenPickerIndex] = useState<number | null>(null);

  return (
    <nav
      aria-label='Breadcrumb'
      className='flex items-center h-[22px] px-3 gap-0.5 border-b border-border bg-breadcrumb-bg shrink-0 overflow-x-auto overflow-y-hidden'
    >
      {
        segments.reduce<{ nodes: React.ReactNode[]; path: string }>(
          (acc, seg, i) => {
            const segPath = acc.path ? `${acc.path}/${seg.label}` : seg.label;
            const isLast = i === segments.length - 1;
            acc.nodes.push(
              <span key={segPath} className='flex items-center gap-0.5 shrink-0'>
                {i > 0 && (
                  <ChevronRight
                    className='h-3 w-3 text-breadcrumb-fg shrink-0'
                    aria-hidden='true'
                  />
                )}
                <SegmentButton
                  seg={seg}
                  isLast={isLast}
                  isOpen={openPickerIndex === i}
                  onOpenChange={(open) => setOpenPickerIndex(open ? i : null)}
                />
              </span>,
            );
            return { nodes: acc.nodes, path: segPath };
          },
          { nodes: [], path: '' },
        ).nodes
      }
    </nav>
  );
}
