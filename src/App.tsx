import { PaneRenderer } from "@/components/panes/PaneRenderer";
import { usePaneStore } from "@/stores/pane-store";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";

function App() {
  const root = usePaneStore((s) => s.root);
  useKeyboardShortcuts();

  return (
    <div className="h-screen w-screen flex flex-col bg-background text-foreground overflow-hidden">
      <PaneRenderer node={root} />
    </div>
  );
}

export default App;
