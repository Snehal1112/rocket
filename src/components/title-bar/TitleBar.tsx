import { type as osType } from '@tauri-apps/plugin-os';
import { Bot, Globe, Settings } from 'lucide-react';
import { useState } from 'react';
import { AssistantToggleButton } from '@/components/assistant/AssistantToggleButton';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
import { ProxySettingsDialog } from '@/components/settings/ProxySettingsDialog';
import { SecretManagerConnectionsDialog } from '@/components/settings/SecretManagerConnectionsDialog';
import { Button } from '@/components/ui/button';
import { WindowControls } from './WindowControls';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export function TitleBar() {
  const isMac = osType() === 'macos';
  const [showSecretManagers, setShowSecretManagers] = useState(false);
  const [showAgentConfigs, setShowAgentConfigs] = useState(false);
  const [showProxy, setShowProxy] = useState(false);

  return (
    <div
      className='relative grid h-10 w-full grid-cols-[1fr_auto_1fr] items-center shrink-0 border-b border-titlebar-border bg-titlebar-bg text-titlebar-fg'
      data-tauri-drag-region
    >
      <div className='flex items-center h-full' data-tauri-drag-region>
        {isMac && <div className='w-[72px] shrink-0' data-tauri-drag-region />}

        <div className='flex items-center gap-2 px-3 shrink-0'>
          <img src='/rocket.png' alt='Rocket' className='h-4 w-4' />
          <span className='text-sm font-medium'>Rocket</span>
        </div>
      </div>

      <WorkspaceSwitcher />

      <div className='flex items-center justify-end h-full gap-1' data-tauri-drag-region>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Secret Manager connections'
          onClick={() => setShowSecretManagers(true)}
        >
          <Settings className='h-4 w-4' aria-hidden='true' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Proxy settings'
          onClick={() => setShowProxy(true)}
        >
          <Globe className='h-4 w-4' aria-hidden='true' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='AI Agent configurations'
          onClick={() => setShowAgentConfigs(true)}
        >
          <Bot className='h-4 w-4' aria-hidden='true' />
        </Button>
        <AssistantToggleButton />
        {!isMac && <WindowControls />}
      </div>

      <SecretManagerConnectionsDialog
        open={showSecretManagers}
        onOpenChange={setShowSecretManagers}
      />
      <AgentConfigsDialog open={showAgentConfigs} onOpenChange={setShowAgentConfigs} />
      <ProxySettingsDialog open={showProxy} onOpenChange={setShowProxy} />
    </div>
  );
}
