import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import { usePaneStore } from '@/stores/pane-store';
import type { PaneNode } from '@/types/pane-types';
import { EditorGroup } from './EditorGroup';

// Recursive entry point that renders leaf groups or resizable split panels.
export function PaneRenderer({ node }: { node: PaneNode }) {
  const resizePane = usePaneStore((s) => s.resizePane);

  if (node.type === 'leaf') {
    return <EditorGroup node={node} />;
  }

  return (
    <ResizablePanelGroup
      orientation={node.direction}
      onLayoutChange={(layout) => {
        const sizes = Object.values(layout);
        if (sizes.length === 2) {
          resizePane(node.id, [sizes[0], sizes[1]]);
        }
      }}
    >
      {/* minSize takes pixels as a plain number in this version of
          react-resizable-panels — unlike defaultSize, where a plain number
          is a percent. A percentage string is required here, confirmed by
          direct measurement while diagnosing the same defect in
          CommitDiffView (minSize={15} silently meant 15px, not 15%). */}
      <ResizablePanel defaultSize={node.sizes[0]} minSize='15%'>
        <PaneRenderer node={node.children[0]} />
      </ResizablePanel>
      <ResizableHandle withHandle />
      <ResizablePanel defaultSize={node.sizes[1]} minSize='15%'>
        <PaneRenderer node={node.children[1]} />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
