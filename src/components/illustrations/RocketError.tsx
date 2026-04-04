// Rocket crashed or broken down with warning signs.
// Best for: error states, failed requests, connection errors.

interface Props {
  className?: string;
}

export function RocketError({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      {/* Background danger glow. */}
      <circle cx='100' cy='105' r='75' className='fill-destructive/5' />

      {/* Ground / surface. */}
      <path
        d='M20 165 Q60 158, 100 162 Q140 166, 180 160 L180 185 H20 Z'
        className='fill-muted/50'
      />

      {/* Smoke clouds. */}
      <circle cx='75' cy='130' r='12' className='fill-muted-foreground/10' />
      <circle cx='90' cy='122' r='10' className='fill-muted-foreground/8' />
      <circle cx='60' cy='125' r='8' className='fill-muted-foreground/7' />
      <circle cx='105' cy='128' r='9' className='fill-muted-foreground/8' />

      {/* Tilted rocket body — crashed at an angle. */}
      <g transform='translate(100, 120) rotate(25)'>
        {/* Body. */}
        <path
          d='M0 -50 C-10 -30, -14 -5, -13 25 L-12 45 H12 L13 25 C14 -5, 10 -30, 0 -50Z'
          className='fill-primary'
        />
        <path
          d='M0 -48 C-6 -30, -8 -5, -8 22 L-7 40 H-2 L-3 22 C-3 -5, -4 -30, 0 -48Z'
          className='fill-white/15'
        />

        {/* Window — cracked. */}
        <circle cx='0' cy='-10' r='8' className='fill-primary-foreground/80' />
        <circle cx='0' cy='-10' r='6' className='fill-background/50' />
        {/* Crack lines. */}
        <line
          x1='-2'
          y1='-14'
          x2='3'
          y2='-7'
          className='stroke-muted-foreground/30'
          strokeWidth='0.8'
        />
        <line
          x1='3'
          y1='-7'
          x2='1'
          y2='-4'
          className='stroke-muted-foreground/30'
          strokeWidth='0.8'
        />
        <line
          x1='3'
          y1='-7'
          x2='5'
          y2='-5'
          className='stroke-muted-foreground/25'
          strokeWidth='0.6'
        />

        {/* Left fin — bent. */}
        <path d='M-13 18 C-20 25, -24 35, -22 42 L-12 38 L-13 18Z' className='fill-primary' />

        {/* Right fin. */}
        <path d='M13 18 C20 25, 24 35, 22 42 L12 38 L13 18Z' className='fill-primary' />

        {/* Nozzle. */}
        <rect x='-8' y='40' width='16' height='5' rx='1.5' className='fill-muted-foreground/40' />
      </g>

      {/* Warning triangle. */}
      <g transform='translate(45, 55)'>
        <path d='M15 0 L30 26 H0Z' className='fill-destructive/80' />
        <path d='M15 3 L27 24 H3Z' className='fill-destructive' />
        {/* Exclamation mark. */}
        <rect x='13.5' y='10' width='3' height='8' rx='1' className='fill-destructive-foreground' />
        <circle cx='15' cy='21' r='1.8' className='fill-destructive-foreground' />
      </g>

      {/* Debris particles. */}
      <rect
        x='130'
        y='100'
        width='4'
        height='4'
        rx='0.5'
        transform='rotate(30, 132, 102)'
        className='fill-muted-foreground/20'
      />
      <rect
        x='140'
        y='115'
        width='3'
        height='3'
        rx='0.5'
        transform='rotate(55, 141, 116)'
        className='fill-muted-foreground/15'
      />
      <rect
        x='65'
        y='95'
        width='3'
        height='3'
        rx='0.5'
        transform='rotate(-20, 66, 96)'
        className='fill-muted-foreground/15'
      />
      <circle cx='145' cy='105' r='2' className='fill-muted-foreground/12' />
      <circle cx='55' y='110' r='1.5' className='fill-muted-foreground/10' />

      {/* Dim stars — things look bleak. */}
      <circle cx='30' cy='35' r='1.5' className='fill-primary/15' />
      <circle cx='170' cy='40' r='1' className='fill-primary/10' />
      <circle cx='165' cy='75' r='1.5' className='fill-primary/12' />
      <circle cx='25' cy='80' r='1' className='fill-primary/10' />
    </svg>
  );
}
