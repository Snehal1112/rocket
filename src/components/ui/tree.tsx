import { ChevronRight } from "lucide-react";
import * as React from "react";
import { cn } from "@/lib/utils";

// Context shared between Tree and all TreeItem descendants.
type TreeContextValue = {
  value: string | undefined;
  onValueChange: (id: string) => void;
};

const TreeContext = React.createContext<TreeContextValue>({
  value: undefined,
  onValueChange: () => {},
});

// Root tree container.
function Tree({
  className,
  value,
  onValueChange,
  children,
  ...props
}: React.ComponentProps<"ul"> & {
  value?: string;
  onValueChange?: (id: string) => void;
}) {
  return (
    <TreeContext.Provider
      value={{ value, onValueChange: onValueChange ?? (() => {}) }}
    >
      <div
        role="tree"
        data-slot="tree"
        className={cn("flex flex-col gap-0.5", className)}
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
  children,
  ...props
}: React.ComponentProps<"li"> & {
  value: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  active?: boolean;
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
    if (
      React.isValidElement(child) &&
      (child.type as React.FC).displayName === "Tree"
    ) {
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
        data-slot="tree-item"
        role="treeitem"
        aria-selected={isSelected}
        aria-expanded={hasChildren ? open : undefined}
        className={cn("flex flex-col", className)}
        tabIndex={-1}
        {...props}
      >
        {/* Clickable row. */}
        <button
          type="button"
          data-slot="tree-item-row"
          data-selected={isSelected || undefined}
          data-active={active || undefined}
          className={cn(
            "flex items-center gap-1 px-1 py-1 text-sm cursor-pointer pl-[var(--tree-indent)] w-full text-left bg-transparent border-0",
            "hover:bg-accent/50",
            "data-[selected]:bg-accent/30",
            "data-[active]:border-l-2 data-[active]:border-primary data-[active]:bg-accent/60 data-[active]:text-accent-foreground",
          )}
          style={
            { "--tree-indent": `${(depth + 1) * 8}px` } as React.CSSProperties
          }
          onClick={handleSelect}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              handleSelect();
            }
          }}
        >
          {/* Expand/collapse chevron, shown only for items with sub-trees. */}
          {hasChildren ? (
            <ChevronRight
              className={cn(
                "size-3 shrink-0 text-muted-foreground transition-transform",
                open && "rotate-90",
              )}
            />
          ) : (
            // Spacer to keep content aligned when there is no chevron.
            <span className="size-3 shrink-0" />
          )}
          {contentChildren}
        </button>

        {/* Nested tree, rendered only when the item is open. */}
        {hasChildren && open && (
          // biome-ignore lint/a11y/useSemanticElements: ul+role=group is standard WAI-ARIA tree widget nested group markup
          <ul role="group" className="flex flex-col gap-0.5">
            {subTreeChildren}
          </ul>
        )}
      </li>
    </TreeItemContext.Provider>
  );
}

// Displayname used above to distinguish Tree children from content children.
Tree.displayName = "Tree";

// Content slot inside a TreeItem row (icon, label, actions, etc.).
function TreeItemContent({
  className,
  children,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="tree-item-content"
      className={cn("flex min-w-0 flex-1 items-center gap-2", className)}
      {...props}
    >
      {children}
    </div>
  );
}

export { Tree, TreeItem, TreeItemContent };
