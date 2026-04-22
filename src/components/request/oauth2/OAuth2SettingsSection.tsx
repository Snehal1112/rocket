import { HelpCircle } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { AuthState } from '@/types/pane-types';

type OAuth2State = NonNullable<AuthState['oauth2']>;

interface OAuth2SettingsSectionProps {
  oauth2: OAuth2State;
  patchOAuth2: (patch: Partial<OAuth2State>) => void;
}

export function OAuth2SettingsSection({ oauth2: o, patchOAuth2 }: OAuth2SettingsSectionProps) {
  return (
    <TooltipProvider>
      <div className='space-y-1.5'>
        <label
          htmlFor='oauth2-auto-fetch'
          className='flex items-center gap-3 py-1 cursor-pointer rounded group'
        >
          <Checkbox
            id='oauth2-auto-fetch'
            checked={o.autoFetchToken}
            onCheckedChange={(checked) => patchOAuth2({ autoFetchToken: !!checked })}
          />
          <span className='text-sm text-foreground/80 group-hover:text-foreground transition-colors select-none flex-1'>
            Automatically fetch token if not found
          </span>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type='button'
                className='shrink-0 rounded focus-visible:outline-2 focus-visible:outline-ring'
                aria-label='Learn more about auto-fetch token'
                tabIndex={0}
              >
                <HelpCircle className='h-3.5 w-3.5 text-muted-foreground hover:text-foreground transition-colors' />
              </button>
            </TooltipTrigger>
            <TooltipContent>
              <p className='text-xs max-w-56 leading-relaxed'>
                When enabled, a token will be fetched automatically before sending a request if no
                token is stored.
              </p>
            </TooltipContent>
          </Tooltip>
        </label>

        <label
          htmlFor='oauth2-auto-refresh'
          className='flex items-center gap-3 py-1 cursor-pointer rounded group'
        >
          <Checkbox
            id='oauth2-auto-refresh'
            checked={o.autoRefreshToken}
            onCheckedChange={(checked) => patchOAuth2({ autoRefreshToken: !!checked })}
          />
          <span className='text-sm text-foreground/80 group-hover:text-foreground transition-colors select-none flex-1'>
            Auto-refresh token when expired
          </span>
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type='button'
                className='shrink-0 rounded focus-visible:outline-2 focus-visible:outline-ring'
                aria-label='Learn more about auto-refresh token'
                tabIndex={0}
              >
                <HelpCircle className='h-3.5 w-3.5 text-muted-foreground hover:text-foreground transition-colors' />
              </button>
            </TooltipTrigger>
            <TooltipContent>
              <p className='text-xs max-w-56 leading-relaxed'>
                When enabled and the token is expired, it will be refreshed automatically using the
                refresh token before sending a request.
              </p>
            </TooltipContent>
          </Tooltip>
        </label>
      </div>
    </TooltipProvider>
  );
}
