import { useState } from 'react';
import { Check, ChevronDown, Settings } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
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
  const globalEnvName = useEnvStore((s) => s.globalEnvName);
  const setGlobalEnv = useEnvStore((s) => s.setGlobalEnv);

  const [dialogOpen, setDialogOpen] = useState(false);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="h-7 gap-1.5 px-2 text-xs">
            {globalEnvName && (
              <span
                className="h-2 w-2 rounded-full bg-teal-500 shrink-0"
                title={`Global: ${globalEnvName}`}
              />
            )}
            <span className={cn(!activeEnvId && 'text-muted-foreground')}>
              {activeEnvId ?? 'No Environment'}
            </span>
            <ChevronDown className="h-3 w-3 opacity-50" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          {/* Global environment section. */}
          <DropdownMenuLabel>Global</DropdownMenuLabel>
          <DropdownMenuItem onClick={() => void setGlobalEnv(null)}>
            <span className="flex-1">No Global Environment</span>
            {!globalEnvName && <Check className="h-3.5 w-3.5" />}
          </DropdownMenuItem>
          {environments.map((env) => (
            <DropdownMenuItem
              key={`global-${env.name}`}
              onClick={() => void setGlobalEnv(env.name)}
            >
              <span className="flex-1">{env.name}</span>
              {globalEnvName === env.name && <Check className="h-3.5 w-3.5" />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          {/* Per-collection environment section. */}
          <DropdownMenuLabel>Environment</DropdownMenuLabel>
          <DropdownMenuItem onClick={() => setActiveEnv(null)}>
            <span className="flex-1">No Environment</span>
            {!activeEnvId && <Check className="h-3.5 w-3.5" />}
          </DropdownMenuItem>
          {environments.map((env) => (
            <DropdownMenuItem
              key={`env-${env.name}`}
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
