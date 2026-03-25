// Gallery page showing all rocket illustrations for visual reference.
// Access via import, or mount as a route/page during development.

import {
  RocketLaunch,
  RocketOrbit,
  RocketIdle,
  RocketSearch,
  RocketError,
  RocketMinimal,
  RocketSleep,
  RocketCloud,
  RocketConstruction,
  RocketStar,
  RocketGroup,
  RocketBook,
  RocketLock,
  RocketSpeed,
  RocketPlanet,
  RocketFolder,
  RocketClock,
  RocketPlug,
  RocketCelebrate,
  RocketTelescope,
} from './index';

const illustrations = [
  { name: 'RocketLaunch', component: RocketLaunch, description: 'Main empty state, welcome screens' },
  { name: 'RocketMinimal', component: RocketMinimal, description: 'Response empty state, send request prompts' },
  { name: 'RocketIdle', component: RocketIdle, description: 'Ready to start, onboarding, first-time prompts' },
  { name: 'RocketOrbit', component: RocketOrbit, description: 'Loading, no results, exploration prompts' },
  { name: 'RocketSearch', component: RocketSearch, description: 'No results found, search/history empty' },
  { name: 'RocketError', component: RocketError, description: 'Error states, failed requests, crashes' },
  { name: 'RocketSleep', component: RocketSleep, description: 'Idle/timeout states, inactive sessions' },
  { name: 'RocketCloud', component: RocketCloud, description: 'Upload, deploy, cloud sync states' },
  { name: 'RocketConstruction', component: RocketConstruction, description: 'Work in progress, maintenance, coming soon' },
  { name: 'RocketStar', component: RocketStar, description: 'Favorites empty, starred items empty' },
  { name: 'RocketGroup', component: RocketGroup, description: 'Team/collaboration empty, workspace sharing' },
  { name: 'RocketBook', component: RocketBook, description: 'Documentation empty, help/guide states' },
  { name: 'RocketLock', component: RocketLock, description: 'Auth required, locked content, permissions' },
  { name: 'RocketSpeed', component: RocketSpeed, description: 'Performance, fast response, speed states' },
  { name: 'RocketPlanet', component: RocketPlanet, description: 'Destination reached, success, completed' },
  { name: 'RocketFolder', component: RocketFolder, description: 'Collections empty, no saved requests' },
  { name: 'RocketClock', component: RocketClock, description: 'History empty, time-related states' },
  { name: 'RocketPlug', component: RocketPlug, description: 'No connection, offline, server unreachable' },
  { name: 'RocketCelebrate', component: RocketCelebrate, description: 'Success, completion, achievement' },
  { name: 'RocketTelescope', component: RocketTelescope, description: 'Explore, discover, browse states' },
];

export function IllustrationGallery() {
  return (
    <div className="p-8">
      <h1 className="text-xl font-semibold mb-6">Rocket Illustrations ({illustrations.length})</h1>
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-6">
        {illustrations.map(({ name, component: Component, description }) => (
          <div key={name} className="border border-border rounded-xl overflow-hidden bg-card">
            <div className="flex items-center justify-center p-6 min-h-[180px]">
              <Component className="w-32 h-32" />
            </div>
            <div className="border-t border-border px-3 py-2.5">
              <p className="text-xs font-semibold">{name}</p>
              <p className="text-[10px] text-muted-foreground mt-0.5">{description}</p>
              <code className="text-[9px] bg-muted px-1 py-0.5 rounded mt-1 inline-block">
                import {'{'} {name} {'}'} from &apos;@/components/illustrations&apos;
              </code>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
