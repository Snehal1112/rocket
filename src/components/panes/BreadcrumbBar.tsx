import { ChevronRight, FileLock, FolderOpen, GitBranch, LayoutDashboard } from 'lucide-react';
import type React from 'react';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
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

interface Segment {
  label: string;
  icon?: React.ReactNode;
}

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
    default:
      return section;
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

function deriveSegments(tab: Tab, workspaceName: string): Segment[] {
  if (isRequestTab(tab)) {
    if (!tab.source) return [{ label: 'Unsaved Request' }];
    const parts = tab.source.path.split('/').filter(Boolean);
    const segments: Segment[] = [{ label: tab.source.collection }];
    for (let i = 0; i < parts.length - 1; i++) {
      segments.push({ label: parts[i], icon: <FolderOpen className='h-3 w-3' /> });
    }
    const name = parts[parts.length - 1] ?? tab.source.path;
    const methodIcon = (
      <span className={`font-semibold text-2xs ${METHOD_TEXT_COLOR[tab.request.method] ?? ''}`}>
        {tab.request.method}
      </span>
    );
    segments.push({ label: name, icon: methodIcon });
    return segments;
  }

  if (isCollectionTab(tab)) {
    return [{ label: tab.collectionName }, { label: collectionSectionLabel(tab.activeSection) }];
  }

  if (isWorkspaceTab(tab)) {
    return [
      { label: workspaceName, icon: <LayoutDashboard className='h-3 w-3' /> },
      { label: workspaceSectionLabel(tab.activeSection) },
    ];
  }

  if (isGitTab(tab)) {
    return [
      { label: tab.collectionName },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
    ];
  }

  if (isDiffTab(tab)) {
    return [
      { label: collectionBasename(tab.diffState.collectionPath) },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.diffState.filePath },
    ];
  }

  if (isConflictTab(tab)) {
    return [
      { label: collectionBasename(tab.conflictState.collectionPath) },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.conflictState.filePath },
    ];
  }

  if (isContractTab(tab)) {
    return [
      { label: tab.collectionName },
      { label: 'Contracts', icon: <FileLock className='h-3 w-3' /> },
    ];
  }

  // Exhaustive: all Tab union members are handled above.
  const _exhaustive: never = tab;
  throw new Error(`Unhandled tab type: ${(_exhaustive as { tabType: string }).tabType}`);
}

interface BreadcrumbBarProps {
  tab: Tab;
}

export function BreadcrumbBar({ tab }: BreadcrumbBarProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspaceName = workspaces.find((w) => w.id === activeWorkspaceId)?.name ?? 'Workspace';

  const segments = deriveSegments(tab, workspaceName);

  return (
    <nav
      aria-label='Breadcrumb'
      className='flex items-center h-7 px-3 gap-1 border-b border-border bg-breadcrumb-bg shrink-0 overflow-x-auto overflow-y-hidden'
    >
      {segments.map((seg, i) => {
        const isLast = i === segments.length - 1;
        return (
          <span key={seg.label} className='flex items-center gap-1 shrink-0'>
            {i > 0 && (
              <ChevronRight className='h-3 w-3 text-breadcrumb-fg shrink-0' aria-hidden='true' />
            )}
            {seg.icon && (
              <span className={isLast ? 'text-breadcrumb-focus-fg' : 'text-breadcrumb-fg'}>
                {seg.icon}
              </span>
            )}
            <span
              className={`text-xs ${isLast ? 'text-breadcrumb-focus-fg' : 'text-breadcrumb-fg'}`}
            >
              {seg.label}
            </span>
          </span>
        );
      })}
    </nav>
  );
}
