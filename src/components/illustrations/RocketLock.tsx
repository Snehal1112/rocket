// Rocket with a lock/shield.
// Best for: auth required, locked content, permission denied.

interface Props { className?: string; }

export function RocketLock({ className = '' }: Props) {
  return (
    <svg viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" className={className}>
      <circle cx="100" cy="100" r="75" className="fill-primary/5" />

      {/* Rocket body. */}
      <g transform="translate(85, 95)">
        <path d="M0 -42 C-10 -24, -13 0, -12 22 L-11 38 H11 L12 22 C13 0, 10 -24, 0 -42Z" className="fill-primary" />
        <path d="M0 -40 C-6 -24, -8 0, -8 20 L-7 34 H-1 L-2 20 C-2 0, -3 -24, 0 -40Z" className="fill-white/15" />
        <circle cx="0" cy="-6" r="7" className="fill-primary-foreground/80" />
        <circle cx="0" cy="-6" r="5.5" className="fill-background/60" />
        <ellipse cx="-1.5" cy="-8" rx="1.5" ry="2" className="fill-white/25" />
        <path d="M-12 18 C-18 24, -20 33, -18 38 L-11 34 L-12 18Z" className="fill-primary" />
        <path d="M12 18 C18 24, 20 33, 18 38 L11 34 L12 18Z" className="fill-primary" />
        <rect x="-7" y="34" width="14" height="5" rx="1.5" className="fill-muted-foreground/35" />
      </g>

      {/* Shield / lock. */}
      <g transform="translate(140, 80)">
        {/* Shield shape. */}
        <path d="M0 -22 L-18 -12 L-18 5 C-18 15, -8 24, 0 28 C8 24, 18 15, 18 5 L18 -12Z" className="fill-primary/25" />
        <path d="M0 -18 L-14 -10 L-14 3 C-14 12, -6 19, 0 22 C6 19, 14 12, 14 3 L14 -10Z" className="fill-primary/15" />
        {/* Lock body. */}
        <rect x="-7" y="-2" width="14" height="12" rx="2" className="fill-muted-foreground/40" />
        {/* Lock shackle. */}
        <path d="M-4 -2 L-4 -8 C-4 -12, 4 -12, 4 -8 L4 -2" className="stroke-muted-foreground/40" strokeWidth="2.5" strokeLinecap="round" />
        {/* Keyhole. */}
        <circle cx="0" cy="4" r="2" className="fill-background/70" />
        <rect x="-1" y="4" width="2" height="4" rx="0.5" className="fill-background/70" />
      </g>

      {/* Stars. */}
      <circle cx="30" cy="40" r="1.5" className="fill-primary/25" />
      <circle cx="40" cy="155" r="1" className="fill-primary/15" />
      <circle cx="170" cy="145" r="1.5" className="fill-primary/18" />
      <circle cx="55" cy="30" r="1" className="fill-primary/20" />
    </svg>
  );
}
