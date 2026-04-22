import { CollectionDropdown } from './CollectionDropdown';
import { EnvironmentSwitcher } from './EnvironmentSwitcher';
import { GitToolbarButton } from './GitToolbarButton';
import { SandboxPopover } from './SandboxPopover';

export function WorkspaceToolbar() {
  return (
    <div className='h-9 border-b border-border bg-card px-3 flex items-center justify-between shrink-0'>
      {/* Left side */}
      <div className='flex items-center gap-2'>
        <CollectionDropdown />
      </div>

      {/* Right side */}
      <div className='flex items-center gap-1'>
        <GitToolbarButton />
        <SandboxPopover />
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
