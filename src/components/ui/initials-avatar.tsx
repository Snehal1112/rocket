import { avatarColorForName, initialsForName } from '@/lib/contracts/avatarColor';

export interface InitialsAvatarProps {
  name: string;
  /** Shown as given. Without it the initials come from the name. */
  initials?: string;
  /** Width and height in pixels. */
  size?: number;
}

/** A round avatar with initials on a colour that depends on the name. */
export function InitialsAvatar({ name, initials, size = 28 }: InitialsAvatarProps) {
  return (
    <span
      aria-hidden='true'
      style={{
        width: size,
        height: size,
        backgroundColor: avatarColorForName(name),
        fontSize: size * 0.4,
      }}
      className='inline-flex shrink-0 select-none items-center justify-center rounded-full font-semibold text-white'
    >
      {initials || initialsForName(name)}
    </span>
  );
}
