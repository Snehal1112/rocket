import { ChevronRight } from 'lucide-react';
import * as React from 'react';
import { cn } from '@/lib/utils';

// Shared metrics so every node type lines up on the same grid.
// Row: 28px tall. Chevron slot: 16px. Icon: 16px. Children are guided by a hairline
// sitting under the parent chevron.
const TREE_CONTENT = 'flex w-full items-center gap-1.5 h-7 px-1.5 text-[13px] cursor-pointer';
const TREE_ICON = 'size-4 shrink-0 text-muted-foreground';
const TREE_ICON_STROKE = 1.75;
const TREE_CHILDREN =
  'ml-[14px] flex flex-col gap-px border-l border-border hover:border-border transition-colors';

// Context shared between Tree and all TreeItem descendants.
type TreeContextValue = {
  value: string | undefined;
  onValueChange: (id: string) => void;
};

const TreeContext = React.createContext<TreeContextValue>({
  value: undefined,
  onValueChange: () => undefined,
});

// Root tree container.
function Tree({
  className,
  value,
  onValueChange,
  children,
  ...props
}: React.ComponentProps<'div'> & {
  value?: string;
  onValueChange?: (id: string) => void;
}) {
  return (
    <TreeContext.Provider value={{ value, onValueChange: onValueChange ?? (() => undefined) }}>
      <div
        role='tree'
        data-slot='tree'
        className={cn('flex flex-col gap-px', className)}
        {...props}
      >
        {children}
      </div>
    </TreeContext.Provider>
  );
}

// Context used by TreeItem to pass depth down to nested trees.
type TreeItemContextValue = {
  depth: number;
};

const TreeItemContext = React.createContext<TreeItemContextValue>({ depth: 0 });

// A single tree node. Handles expand/collapse when children are present.
function TreeItem({
  value,
  className,
  open: openProp,
  onOpenChange,
  active,
  leading = 'auto',
  children,
  ...props
}: React.ComponentProps<'li'> & {
  value: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  active?: boolean;
  // 'none' means the node draws its own chevron, so the item adds no leading slot.
  leading?: 'auto' | 'none';
}) {
  const [openState, setOpenState] = React.useState(false);
  // Use controlled open when the caller provides it, otherwise fall back to
  // internal state (uncontrolled mode).
  const isControlled = openProp !== undefined;
  const open = isControlled ? openProp : openState;
  const { value: selectedValue, onValueChange } = React.useContext(TreeContext);
  const { depth } = React.useContext(TreeItemContext);

  const isSelected = selectedValue === value;

  // Separate TreeItemContent children from nested Tree children so we can
  // render the expand toggle only when sub-trees exist.
  const contentChildren: React.ReactNode[] = [];
  const subTreeChildren: React.ReactNode[] = [];

  React.Children.forEach(children, (child) => {
    if (React.isValidElement(child) && (child.type as React.FC).displayName === 'Tree') {
      subTreeChildren.push(child);
    } else {
      contentChildren.push(child);
    }
  });

  const hasChildren = subTreeChildren.length > 0;

  function handleSelect() {
    onValueChange(value);
    if (hasChildren) {
      const next = !open;
      if (!isControlled) setOpenState(next);
      onOpenChange?.(next);
    }
  }

  return (
    <TreeItemContext.Provider value={{ depth: depth + 1 }}>
      <li
        data-slot='tree-item'
        role='treeitem'
        aria-selected={isSelected}
        aria-expanded={hasChildren ? open : undefined}
        className={cn('flex flex-col', className)}
        tabIndex={-1}
        {...props}
      >
        {/* Clickable row. */}
        <button
          type='button'
          data-slot='tree-item-row'
          data-selected={isSelected || undefined}
          data-active={active || undefined}
          className={cn(
            'relative flex items-center w-full min-w-0 text-left bg-transparent rounded-sm cursor-pointer',
            'hover:bg-list-hover-bg',
            'data-selected:bg-list-inactive-bg',
            'data-active:bg-list-active-bg data-active:text-list-active-fg',
            // Accent bar for the open item, inset so the row keeps its rounded corners.
            'data-active:shadow-[inset_2px_0_0_hsl(var(--list-highlight-fg))]',
            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring',
          )}
          onClick={handleSelect}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              handleSelect();
            }
          }}
        >
          {/* Leading slot: chevron for items with sub-trees, a spacer for leaves. */}
          {leading === 'auto' &&
            (hasChildren ? (
              <TreeChevron open={open} className='ml-1.5 -mr-1' />
            ) : (
              <span className='ml-1.5 -mr-1 size-4 shrink-0' />
            ))}
          {contentChildren}
        </button>

        {/* Nested tree, rendered only when the item is open. */}
        {hasChildren && open && (
          // biome-ignore lint/a11y/useSemanticElements: ul+role=group is standard WAI-ARIA tree widget nested group markup
          <ul role='group' className={TREE_CHILDREN}>
            {subTreeChildren}
          </ul>
        )}
      </li>
    </TreeItemContext.Provider>
  );
}

// Single chevron used by every expandable node.
function TreeChevron({ open, className }: { open: boolean; className?: string }) {
  return (
    <span
      aria-hidden='true'
      className={cn('flex size-4 shrink-0 items-center justify-center', className)}
    >
      <ChevronRight
        className={cn(
          'size-3.5 text-muted-foreground transition-transform duration-150',
          open && 'rotate-90',
        )}
        strokeWidth={2}
      />
    </span>
  );
}

// Displayname used above to distinguish Tree children from content children.
Tree.displayName = 'Tree';

// Content slot inside a TreeItem row (icon, label, actions, etc.).
function TreeItemContent({ className, children, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot='tree-item-content'
      className={cn(TREE_CONTENT, 'min-w-0 flex-1', className)}
      {...props}
    >
      {children}
    </div>
  );
}

export {
  TREE_CHILDREN,
  TREE_CONTENT,
  TREE_ICON,
  TREE_ICON_STROKE,
  Tree,
  TreeChevron,
  TreeItem,
  TreeItemContent,
};
