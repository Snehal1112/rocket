import { useState } from 'react';
import { Check, ChevronDown, Settings } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useEnvStore } from '@/stores/env-store';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';

export function EnvironmentSwitcher() {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnv = useEnvStore((s) => s.setActiveEnv);

  const [dialogOpen, setDialogOpen] = useState(false);

  const activeName = activeEnvId ?? 'No Environment';

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1.5 px-2 text-sm font-normal"
          >
            <span
              className={cn(
                'h-2 w-2 rounded-full',
                activeEnvId ? 'bg-green-500 dark:bg-green-400' : 'bg-muted-foreground/50',
              )}
            />
            <span className="max-w-[120px] truncate">{activeName}</span>
            <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          <DropdownMenuItem onClick={() => setActiveEnv(null)}>
            <span className="flex-1">No Environment</span>
            {!activeEnvId && <Check className="h-3.5 w-3.5" />}
          </DropdownMenuItem>
          {environments.length > 0 && <DropdownMenuSeparator />}
          {environments.map((env) => (
            <DropdownMenuItem
              key={env.name}
              onClick={() => setActiveEnv(env.name)}
            >
              <span className="flex-1">{env.name}</span>
              {activeEnvId === env.name && <Check className="h-3.5 w-3.5" />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => setDialogOpen(true)}>
            <Settings className="h-3.5 w-3.5 mr-2" />
            Manage Environments...
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <EnvironmentDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  );
}
