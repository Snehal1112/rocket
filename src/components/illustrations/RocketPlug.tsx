// Rocket with a disconnected plug.
// Best for: no connection, offline, server unreachable.

interface Props {
  className?: string;
}

export function RocketPlug({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      <circle cx='100' cy='100' r='75' className='fill-primary/5' />

      {/* Rocket. */}
      <g transform='translate(80, 90)'>
        <path
          d='M0 -35 C-9 -18, -11 2, -11 20 L-10 30 H10 L11 20 C11 2, 9 -18, 0 -35Z'
          className='fill-primary'
        />
        <path
          d='M0 -33 C-5 -18, -7 2, -7 18 L-6 27 H-1 L-1.5 18 C-1.5 2, -2 -18, 0 -33Z'
          className='fill-white/15'
        />
        <circle cx='0' cy='-3' r='6.5' className='fill-primary-foreground/80' />
        <circle cx='0' cy='-3' r='5' className='fill-background/60' />
        <ellipse cx='-1.5' cy='-5' rx='1.2' ry='1.8' className='fill-white/25' />
        <path d='M-11 16 C-16 22, -18 30, -16 34 L-10 30 L-11 16Z' className='fill-primary' />
        <path d='M11 16 C16 22, 18 30, 16 34 L10 30 L11 16Z' className='fill-primary' />
        <rect x='-6' y='28' width='12' height='4' rx='1.2' className='fill-muted-foreground/35' />
      </g>

      {/* Disconnected plug — left side (from rocket). */}
      <g transform='translate(110, 100)'>
        {/* Cable. */}
        <path
          d='M0 0 C8 -5, 15 -2, 18 0'
          className='stroke-muted-foreground/30'
          strokeWidth='3'
          strokeLinecap='round'
        />
        {/* Plug head. */}
        <rect x='16' y='-6' width='12' height='12' rx='2' className='fill-muted-foreground/30' />
        {/* Plug prongs. */}
        <rect x='28' y='-4' width='8' height='3' rx='1' className='fill-muted-foreground/35' />
        <rect x='28' y='1' width='8' height='3' rx='1' className='fill-muted-foreground/35' />
      </g>

      {/* Socket — right side (disconnected). */}
      <g transform='translate(148, 100)'>
        <rect x='0' y='-8' width='14' height='16' rx='2' className='fill-muted-foreground/20' />
        <rect x='2' y='-4' width='4' height='3' rx='0.8' className='fill-background/50' />
        <rect x='2' y='1' width='4' height='3' rx='0.8' className='fill-background/50' />
      </g>

      {/* Disconnection gap — lightning bolt. */}
      <g className='fill-yellow-400/40'>
        <path d='M140 90 L137 97 L141 97 L138 104' />
      </g>

      {/* Small X marks indicating no connection. */}
      <g className='stroke-destructive/30' strokeWidth='1.5' strokeLinecap='round'>
        <line x1='130' y1='115' x2='134' y2='119' />
        <line x1='134' y1='115' x2='130' y2='119' />
      </g>

      {/* Stars. */}
      <circle cx='30' cy='40' r='1.5' className='fill-primary/25' />
      <circle cx='170' cy='40' r='1' className='fill-primary/20' />
      <circle cx='35' cy='150' r='1' className='fill-primary/15' />
      <circle cx='165' cy='155' r='1.5' className='fill-primary/18' />
    </svg>
  );
}
