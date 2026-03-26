import { EnvironmentSwitcher } from '@/components/layout/EnvironmentSwitcher';

export function StatusBar() {
  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <div className="ml-auto">
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
