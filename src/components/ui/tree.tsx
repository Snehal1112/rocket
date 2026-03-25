import * as React from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

// Context shared between Tree and all TreeItem descendants.
type TreeContextValue = {
  selectedId: string | undefined;
  onSelect: (id: string) => void;
};

const TreeContext = React.createContext<TreeContextValue>({
  selectedId: undefined,
  onSelect: () => {},
});

// Root tree container.
function Tree({
  className,
  selectedId,
  onSelect,
  children,
  ...props
}: React.ComponentProps<"ul"> & {
  selectedId?: string;
  onSelect?: (id: string) => void;
}) {
  return (
    <TreeContext.Provider
      value={{ selectedId, onSelect: onSelect ?? (() => {}) }}
    >
      <ul
        data-slot="tree"
        role="tree"
        className={cn("flex flex-col gap-0.5", className)}
        {...props}
      >
        {children}
      </ul>
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
  id,
  className,
  children,
  ...props
}: React.ComponentProps<"li"> & {
  id: string;
}) {
  const [open, setOpen] = React.useState(false);
  const { selectedId, onSelect } = React.useContext(TreeContext);
  const { depth } = React.useContext(TreeItemContext);

  const isSelected = selectedId === id;

  // Separate TreeItemContent children from nested Tree children so we can
  // render the expand toggle only when sub-trees exist.
  const contentChildren: React.ReactNode[] = [];
  const subTreeChildren: React.ReactNode[] = [];

  React.Children.forEach(children, (child) => {
    if (React.isValidElement(child) && (child.type as React.FC).displayName === "Tree") {
      subTreeChildren.push(child);
    } else {
      contentChildren.push(child);
    }
  });

  const hasChildren = subTreeChildren.length > 0;

  function handleSelect() {
    onSelect(id);
    if (hasChildren) setOpen((prev) => !prev);
  }

  return (
    <TreeItemContext.Provider value={{ depth: depth + 1 }}>
      <li
        data-slot="tree-item"
        role="treeitem"
        aria-selected={isSelected}
        aria-expanded={hasChildren ? open : undefined}
        className={cn("flex flex-col", className)}
        {...props}
      >
        {/* Clickable row. */}
        <div
          data-slot="tree-item-row"
          data-selected={isSelected || undefined}
          className={cn(
            "flex items-center gap-1 rounded-md px-2 py-1 text-sm cursor-pointer",
            "hover:bg-accent hover:text-accent-foreground",
            "data-[selected]:bg-accent data-[selected]:text-accent-foreground",
          )}
          style={{ paddingLeft: `${(depth + 1) * 12}px` }}
          onClick={handleSelect}
        >
          {/* Expand/collapse chevron, shown only for items with sub-trees. */}
          {hasChildren ? (
            <ChevronRight
              className={cn(
                "size-4 shrink-0 text-muted-foreground transition-transform",
                open && "rotate-90",
              )}
            />
          ) : (
            // Spacer to keep content aligned when there is no chevron.
            <span className="size-4 shrink-0" />
          )}
          {contentChildren}
        </div>

        {/* Nested tree, rendered only when the item is open. */}
        {hasChildren && open && (
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
