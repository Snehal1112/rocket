import { HelpCircle } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
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
      <div className='space-y-2'>
        <Label className='mb-1 block'>Settings</Label>
        <div className='space-y-2 pl-1'>
          <div className='flex items-center gap-2'>
            <Checkbox
              id='oauth2-auto-fetch'
              checked={o.autoFetchToken}
              onCheckedChange={(checked) => patchOAuth2({ autoFetchToken: !!checked })}
            />
            <Label
              htmlFor='oauth2-auto-fetch'
              className='text-xs text-muted-foreground cursor-pointer'
            >
              Automatically fetch token if not found
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <HelpCircle className='h-3 w-3 text-muted-foreground cursor-help' />
              </TooltipTrigger>
              <TooltipContent>
                <p className='text-xs max-w-52'>
                  When enabled, a token will be fetched automatically before sending a request if no
                  token is stored.
                </p>
              </TooltipContent>
            </Tooltip>
          </div>
          <div className='flex items-center gap-2'>
            <Checkbox
              id='oauth2-auto-refresh'
              checked={o.autoRefreshToken}
              onCheckedChange={(checked) => patchOAuth2({ autoRefreshToken: !!checked })}
            />
            <Label
              htmlFor='oauth2-auto-refresh'
              className='text-xs text-muted-foreground cursor-pointer'
            >
              Auto refresh token (with refresh URL)
            </Label>
            <Tooltip>
              <TooltipTrigger asChild>
                <HelpCircle className='h-3 w-3 text-muted-foreground cursor-help' />
              </TooltipTrigger>
              <TooltipContent>
                <p className='text-xs max-w-52'>
                  When enabled and the token is expired, it will be refreshed automatically using
                  the refresh token before sending a request.
                </p>
              </TooltipContent>
            </Tooltip>
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}
