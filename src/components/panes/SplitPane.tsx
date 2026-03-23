import type { SplitNode } from '@/types/pane-types';
import { usePaneStore } from '@/stores/pane-store';
import { PaneRenderer } from './PaneRenderer';
import { ResizeHandle } from './ResizeHandle';

// Renders two children with a draggable resize handle between them.
export function SplitPane({ node }: { node: SplitNode }) {
  const resizePane = usePaneStore((s) => s.resizePane);
  const isHorizontal = node.direction === 'horizontal';

  return (
    <div
      className={`flex h-full w-full ${isHorizontal ? 'flex-row' : 'flex-col'}`}
    >
      <div
        className="min-h-0 min-w-0 overflow-hidden"
        style={{ flexBasis: `${node.sizes[0]}%`, flexGrow: 0, flexShrink: 0 }}
      >
        <PaneRenderer node={node.children[0]} />
      </div>

      <ResizeHandle
        direction={node.direction}
        onResize={(sizes) => resizePane(node.id, sizes)}
      />

      <div
        className="min-h-0 min-w-0 overflow-hidden"
        style={{ flexBasis: `${node.sizes[1]}%`, flexGrow: 0, flexShrink: 0 }}
      >
        <PaneRenderer node={node.children[1]} />
      </div>
    </div>
  );
}
