import { cn } from '@/lib/utils';

export interface BrunoTabDef {
  value: string;
  label: React.ReactNode;
  isActive: boolean;
  onClick: () => void;
}

interface BrunoTabBarProps {
  tabs: BrunoTabDef[];
  rightContent?: React.ReactNode;
}

export function BrunoTabBar({ tabs, rightContent }: BrunoTabBarProps) {
  return (
    <div className="flex items-center border-b border-border px-3 shrink-0">
      {tabs.map((tab) => (
        <button
          key={tab.value}
          type="button"
          onClick={tab.onClick}
          className={cn(
            'py-2 mr-4 text-sm border-b-2 -mb-px transition-colors',
            tab.isActive
              ? 'border-primary text-foreground font-semibold'
              : 'border-transparent text-muted-foreground hover:text-foreground',
          )}
        >
          {tab.label}
        </button>
      ))}
      {rightContent && (
        <div className="ml-auto flex items-center gap-2">
          {rightContent}
        </div>
      )}
    </div>
  );
}
