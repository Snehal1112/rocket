// Rocket launching upward with full exhaust trail and stars.
// Best for: main empty state, welcome screens.

interface Props {
  className?: string;
}

export function RocketLaunch({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      {/* Background glow. */}
      <circle cx='100' cy='100' r='80' className='fill-primary/5' />
      <circle cx='100' cy='100' r='55' className='fill-primary/8' />

      {/* Exhaust trail / flame. */}
      <path
        d='M92 148 C88 158, 85 172, 88 185 Q100 175, 112 185 C115 172, 112 158, 108 148'
        className='fill-orange-400/70'
      />
      <path
        d='M95 148 C93 155, 91 165, 94 175 Q100 168, 106 175 C109 165, 107 155, 105 148'
        className='fill-yellow-400/80'
      />

      {/* Rocket body. */}
      <path
        d='M100 30 C85 55, 78 85, 80 120 L82 145 H118 L120 120 C122 85, 115 55, 100 30Z'
        className='fill-primary'
      />

      {/* Body highlight. */}
      <path
        d='M100 35 C90 55, 85 80, 86 110 L87 135 H95 L94 110 C93 80, 95 55, 100 35Z'
        className='fill-white/20'
      />

      {/* Nose cone accent. */}
      <path d='M100 30 C94 45, 90 58, 88 72 C96 58, 100 45, 100 30Z' className='fill-white/10' />

      {/* Window. */}
      <circle cx='100' cy='80' r='12' className='fill-primary-foreground/90' />
      <circle cx='100' cy='80' r='9' className='fill-background/60' />
      <ellipse cx='97' cy='77' rx='3' ry='4' className='fill-white/30' />

      {/* Left fin. */}
      <path d='M80 118 C70 125, 62 140, 65 155 L82 145 L80 118Z' className='fill-primary' />
      <path d='M80 118 C74 125, 68 137, 67 148 L75 142 L80 118Z' className='fill-white/15' />

      {/* Right fin. */}
      <path d='M120 118 C130 125, 138 140, 135 155 L118 145 L120 118Z' className='fill-primary' />

      {/* Nozzle. */}
      <rect x='85' y='140' width='30' height='8' rx='2' className='fill-muted-foreground/40' />
      <rect x='88' y='143' width='24' height='5' rx='1.5' className='fill-muted-foreground/25' />

      {/* Stars. */}
      <circle cx='45' cy='55' r='2' className='fill-primary/40' />
      <circle cx='155' cy='45' r='1.5' className='fill-primary/30' />
      <circle cx='35' cy='105' r='1.5' className='fill-primary/25' />
      <circle cx='165' cy='95' r='2' className='fill-primary/35' />
      <circle cx='50' cy='155' r='1' className='fill-primary/20' />
      <circle cx='150' cy='150' r='1.5' className='fill-primary/25' />
      <circle cx='140' cy='60' r='1' className='fill-primary/30' />
      <circle cx='60' cy='40' r='1' className='fill-primary/20' />

      {/* Speed lines. */}
      <line
        x1='70'
        y1='160'
        x2='70'
        y2='170'
        className='stroke-muted-foreground/20'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='130'
        y1='162'
        x2='130'
        y2='172'
        className='stroke-muted-foreground/20'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='55'
        y1='140'
        x2='55'
        y2='148'
        className='stroke-muted-foreground/15'
        strokeWidth='1'
        strokeLinecap='round'
      />
      <line
        x1='145'
        y1='138'
        x2='145'
        y2='146'
        className='stroke-muted-foreground/15'
        strokeWidth='1'
        strokeLinecap='round'
      />
    </svg>
  );
}
