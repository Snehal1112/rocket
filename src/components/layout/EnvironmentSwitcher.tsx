import { Check, ChevronDown, Database, Globe, Plus, Settings } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

export function EnvironmentSwitcher() {
  const environments = useEnvStore((s) => s.environments);
  const activeEnvId = useEnvStore((s) => s.activeEnvId);
  const setActiveEnv = useEnvStore((s) => s.setActiveEnv);
  const globalEnvName = useEnvStore((s) => s.globalEnvName);
  const setGlobalEnv = useEnvStore((s) => s.setGlobalEnv);
  const globalEnvironments = useEnvStore((s) => s.globalEnvironments);
  const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);
  const createGlobalEnvironment = useEnvStore((s) => s.createGlobalEnvironment);
  const createEnvironment = useEnvStore((s) => s.createEnvironment);

  const [popoverOpen, setPopoverOpen] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [isCreatingGlobal, setIsCreatingGlobal] = useState(false);
  const [newGlobalName, setNewGlobalName] = useState('');
  const [isCreatingCollection, setIsCreatingCollection] = useState(false);
  const [newCollectionEnvName, setNewCollectionEnvName] = useState('');
  const createGlobalInFlight = useRef(false);
  const createCollectionInFlight = useRef(false);

  // Load global environments when popover opens if not already loaded.
  useEffect(() => {
    if (popoverOpen && globalEnvironments.length === 0) {
      loadGlobalEnvironments().catch(console.error);
    }
  }, [popoverOpen, globalEnvironments.length, loadGlobalEnvironments]);

  const handleCreateGlobal = async () => {
    if (createGlobalInFlight.current) return;
    const name = newGlobalName.trim();
    if (!name) {
      setIsCreatingGlobal(false);
      setNewGlobalName('');
      return;
    }
    createGlobalInFlight.current = true;
    try {
      await createGlobalEnvironment(name);
    } catch (err) {
      console.error('[EnvironmentSwitcher] create global env failed:', err);
    } finally {
      createGlobalInFlight.current = false;
      setNewGlobalName('');
      setIsCreatingGlobal(false);
    }
  };

  const handleCreateCollection = async () => {
    if (createCollectionInFlight.current) return;
    const name = newCollectionEnvName.trim();
    if (!name) {
      setIsCreatingCollection(false);
      setNewCollectionEnvName('');
      return;
    }
    createCollectionInFlight.current = true;
    try {
      await createEnvironment(name);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes('No active collection')) {
        toast.warning('Open a collection first to create collection environments.');
      } else {
        toast.error(`Failed to create environment: ${msg}`);
      }
    } finally {
      createCollectionInFlight.current = false;
      setNewCollectionEnvName('');
      setIsCreatingCollection(false);
    }
  };

  const handleOpenGlobalConfigure = () => {
    setPopoverOpen(false);
    const activeId = useWorkspaceStore.getState().activeWorkspaceId;
    if (!activeId) return;
    usePaneStore.getState().openWorkspaceTabs(activeId);
    // Activate the environments tab.
    const envTabId = `workspace:${activeId}:environments`;
    const { root } = usePaneStore.getState();
    const activateEnvTab = (node: typeof root): void => {
      if (node.type === 'leaf') {
        const found = node.tabs.find((t) => t.id === envTabId);
        if (found) {
          usePaneStore.getState().setActiveTab(envTabId, node.groupId);
        }
      } else {
        activateEnvTab(node.children[0]);
        activateEnvTab(node.children[1]);
      }
    };
    activateEnvTab(usePaneStore.getState().root);
  };

  return (
    <>
      <Popover open={popoverOpen} onOpenChange={setPopoverOpen}>
        <PopoverTrigger asChild>
          <Button
            variant='ghost'
            size='sm'
            className='h-7 gap-2.5 px-2 text-xs hover:bg-toolbar-hover'
            aria-label='Switch environment'
          >
            {activeEnvId && (
              <span className='flex items-center gap-1'>
                <Database className='h-3 w-3 text-muted-foreground shrink-0' />
                <span className='max-w-[80px] truncate'>{activeEnvId}</span>
              </span>
            )}
            {globalEnvName && (
              <span className='flex items-center gap-1'>
                <Globe className='h-3 w-3 text-muted-foreground shrink-0' />
                <span className='max-w-[80px] truncate'>{globalEnvName}</span>
              </span>
            )}
            {!activeEnvId && !globalEnvName && (
              <span className='text-muted-foreground'>No Environment</span>
            )}
            <ChevronDown className='h-3 w-3 opacity-50' aria-hidden='true' />
          </Button>
        </PopoverTrigger>
        <PopoverContent align='start' className='w-56 p-0'>
          <Tabs defaultValue='collection'>
            <TabsList className='w-full rounded-none border-b bg-transparent px-2 pt-1 pb-0 h-auto'>
              <TabsTrigger
                value='collection'
                className='flex-1 text-xs h-7 rounded-b-none data-[state=active]:shadow-none data-[state=active]:border-b-2 data-[state=active]:border-primary'
              >
                Collection
              </TabsTrigger>
              <TabsTrigger
                value='global'
                className='flex-1 text-xs h-7 rounded-b-none data-[state=active]:shadow-none data-[state=active]:border-b-2 data-[state=active]:border-primary'
              >
                <Globe className='h-3 w-3 mr-1' />
                Global
              </TabsTrigger>
            </TabsList>

            {/* Collection-scoped environments. */}
            <TabsContent value='collection' className='mt-0'>
              {environments.length === 0 && !isCreatingCollection ? (
                <div className='flex flex-col items-center gap-2 px-4 py-5 text-center'>
                  <p className='text-xs font-medium'>Ready to get started?</p>
                  <p className='text-[11px] text-muted-foreground'>
                    Create your first environment to begin working with your collection.
                  </p>
                  <div className='flex flex-col gap-1.5 w-full mt-1'>
                    <Button
                      variant='outline'
                      size='sm'
                      className='w-full text-xs h-7'
                      onClick={() => setIsCreatingCollection(true)}
                    >
                      <Plus className='h-3 w-3 mr-1.5' />
                      Create
                    </Button>
                    <Button
                      variant='outline'
                      size='sm'
                      className='w-full text-xs h-7'
                      onClick={() => {
                        setPopoverOpen(false);
                        setDialogOpen(true);
                      }}
                    >
                      <Settings className='h-3 w-3 mr-1.5' />
                      Configure
                    </Button>
                  </div>
                </div>
              ) : (
                <>
                  <div className='py-1' role='radiogroup' aria-label='Active environment'>
                    {/* biome-ignore lint/a11y/useSemanticElements: styled button with click handler, not a native radio */}
                    <button
                      type='button'
                      role='radio'
                      aria-checked={activeEnvId === null}
                      className='flex items-center gap-2 w-full px-3 py-1.5 text-xs hover:bg-muted/50 cursor-pointer'
                      onClick={() => {
                        setActiveEnv(null);
                        setPopoverOpen(false);
                      }}
                    >
                      <Check
                        className='h-3 w-3 shrink-0'
                        aria-hidden='true'
                        style={{ opacity: activeEnvId === null ? 1 : 0 }}
                      />
                      <span className='text-muted-foreground'>No Environment</span>
                    </button>
                    {environments.map((env) => (
                      // biome-ignore lint/a11y/useSemanticElements: styled button with click handler
                      <button
                        type='button'
                        role='radio'
                        aria-checked={activeEnvId === env.name}
                        key={env.name}
                        className='flex items-center gap-2 w-full px-3 py-1.5 text-xs hover:bg-muted/50 cursor-pointer'
                        onClick={() => {
                          setActiveEnv(env.name);
                          setPopoverOpen(false);
                        }}
                      >
                        <Check
                          className='h-3 w-3 shrink-0'
                          aria-hidden='true'
                          style={{ opacity: activeEnvId === env.name ? 1 : 0 }}
                        />
                        <span className='flex-1 text-left truncate'>{env.name}</span>
                      </button>
                    ))}
                    {isCreatingCollection && (
                      <div className='px-3 py-1'>
                        <Input
                          autoFocus
                          className='h-6 text-xs'
                          placeholder='Environment name'
                          value={newCollectionEnvName}
                          onChange={(e) => setNewCollectionEnvName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') void handleCreateCollection();
                            if (e.key === 'Escape') {
                              setIsCreatingCollection(false);
                              setNewCollectionEnvName('');
                            }
                          }}
                          onBlur={() => void handleCreateCollection()}
                        />
                      </div>
                    )}
                  </div>
                  <div className='border-t px-3 py-1.5 flex items-center justify-between'>
                    <button
                      type='button'
                      className='flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground cursor-pointer'
                      onClick={() => {
                        setPopoverOpen(false);
                        setDialogOpen(true);
                      }}
                    >
                      <Settings className='h-3 w-3' />
                      Configure
                    </button>
                    {!isCreatingCollection && (
                      <button
                        type='button'
                        className='flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground cursor-pointer'
                        onClick={() => setIsCreatingCollection(true)}
                        aria-label='Add collection environment'
                      >
                        <Plus className='h-3 w-3' />
                      </button>
                    )}
                  </div>
                </>
              )}
            </TabsContent>

            {/* Global (workspace-level) environments. */}
            <TabsContent value='global' className='mt-0'>
              {globalEnvironments.length === 0 && !isCreatingGlobal ? (
                <div className='flex flex-col items-center gap-2 px-4 py-5 text-center'>
                  <p className='text-xs font-medium'>Ready to get started?</p>
                  <p className='text-[11px] text-muted-foreground'>
                    Create your first global environment to begin working across collections.
                  </p>
                  <div className='flex flex-col gap-1.5 w-full mt-1'>
                    <Button
                      variant='outline'
                      size='sm'
                      className='w-full text-xs h-7'
                      onClick={() => setIsCreatingGlobal(true)}
                    >
                      <Plus className='h-3 w-3 mr-1.5' />
                      Create
                    </Button>
                    <Button
                      variant='outline'
                      size='sm'
                      className='w-full text-xs h-7'
                      onClick={handleOpenGlobalConfigure}
                    >
                      <Settings className='h-3 w-3 mr-1.5' />
                      Configure
                    </Button>
                  </div>
                </div>
              ) : (
                <>
                  <div className='py-1' role='radiogroup' aria-label='Active global environment'>
                    {/* biome-ignore lint/a11y/useSemanticElements: styled button with click handler */}
                    <button
                      type='button'
                      role='radio'
                      aria-checked={globalEnvName === null}
                      className='flex items-center gap-2 w-full px-3 py-1.5 text-xs hover:bg-muted/50 cursor-pointer'
                      onClick={() => {
                        void setGlobalEnv(null);
                        setPopoverOpen(false);
                      }}
                    >
                      <Check
                        className='h-3 w-3 shrink-0'
                        aria-hidden='true'
                        style={{ opacity: globalEnvName === null ? 1 : 0 }}
                      />
                      <span className='text-muted-foreground'>No Global Environment</span>
                    </button>
                    {globalEnvironments.map((env) => (
                      // biome-ignore lint/a11y/useSemanticElements: styled button with click handler
                      <button
                        type='button'
                        role='radio'
                        aria-checked={globalEnvName === env.name}
                        key={env.name}
                        className='flex items-center gap-2 w-full px-3 py-1.5 text-xs hover:bg-muted/50 cursor-pointer'
                        onClick={() => {
                          void setGlobalEnv(env.name);
                          setPopoverOpen(false);
                        }}
                      >
                        <Check
                          className='h-3 w-3 shrink-0'
                          aria-hidden='true'
                          style={{ opacity: globalEnvName === env.name ? 1 : 0 }}
                        />
                        <span className='flex-1 text-left truncate'>{env.name}</span>
                      </button>
                    ))}
                    {isCreatingGlobal && (
                      <div className='px-3 py-1'>
                        <Input
                          autoFocus
                          className='h-6 text-xs'
                          placeholder='Environment name'
                          value={newGlobalName}
                          onChange={(e) => setNewGlobalName(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') void handleCreateGlobal();
                            if (e.key === 'Escape') {
                              setIsCreatingGlobal(false);
                              setNewGlobalName('');
                            }
                          }}
                          onBlur={() => void handleCreateGlobal()}
                        />
                      </div>
                    )}
                  </div>
                  <div className='border-t px-3 py-1.5 flex items-center justify-between'>
                    <button
                      type='button'
                      className='flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground cursor-pointer'
                      onClick={handleOpenGlobalConfigure}
                    >
                      <Settings className='h-3 w-3' />
                      Configure
                    </button>
                    {!isCreatingGlobal && (
                      <button
                        type='button'
                        className='flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground cursor-pointer'
                        onClick={() => setIsCreatingGlobal(true)}
                        aria-label='Add global environment'
                      >
                        <Plus className='h-3 w-3' />
                      </button>
                    )}
                  </div>
                </>
              )}
            </TabsContent>
          </Tabs>
        </PopoverContent>
      </Popover>
      <EnvironmentDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  );
}
