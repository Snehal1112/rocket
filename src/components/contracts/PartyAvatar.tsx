import { avatarColorForName, initialsForName } from '@/lib/contracts/avatarColor';
import type { Party } from '@/types/contracts';

interface PartyAvatarProps {
  party: Party;
  size?: number;
}

export function PartyAvatar({ party, size = 20 }: PartyAvatarProps) {
  const bg = avatarColorForName(party.name, party.avatarColor);
  const initials = initialsForName(party.name);
  return (
    <span
      aria-hidden='true'
      style={{ width: size, height: size, backgroundColor: bg, fontSize: size * 0.4 }}
      className='rounded-full inline-flex items-center justify-center text-white font-semibold shrink-0 select-none'
    >
      {initials}
    </span>
  );
}
