// Rocket with a telescope — discovery.
// Best for: explore, discover, browse states.

interface Props {
  className?: string;
}

export function RocketTelescope({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      <circle cx='100' cy='100' r='75' className='fill-primary/5' />

      {/* Telescope. */}
      <g transform='translate(55, 115) rotate(-35)'>
        {/* Tripod legs. */}
        <line
          x1='0'
          y1='15'
          x2='-12'
          y2='40'
          className='stroke-muted-foreground/25'
          strokeWidth='2'
          strokeLinecap='round'
        />
        <line
          x1='0'
          y1='15'
          x2='12'
          y2='40'
          className='stroke-muted-foreground/25'
          strokeWidth='2'
          strokeLinecap='round'
        />
        <line
          x1='0'
          y1='15'
          x2='0'
          y2='42'
          className='stroke-muted-foreground/25'
          strokeWidth='2'
          strokeLinecap='round'
        />
        {/* Telescope tube. */}
        <rect x='-4' y='-25' width='8' height='40' rx='3' className='fill-muted-foreground/30' />
        {/* Lens. */}
        <ellipse cx='0' cy='-27' rx='6' ry='3' className='fill-primary/25' />
        <ellipse cx='0' cy='-27' rx='4' ry='2' className='fill-primary/15' />
        {/* Eyepiece. */}
        <rect x='-3' y='15' width='6' height='5' rx='1.5' className='fill-muted-foreground/25' />
      </g>

      {/* What the telescope sees — a distant star cluster. */}
      <circle cx='45' cy='40' r='3' className='fill-yellow-400/40' />
      <circle cx='38' cy='48' r='1.5' className='fill-yellow-400/25' />
      <circle cx='52' cy='45' r='1' className='fill-yellow-400/20' />
      <circle cx='42' cy='35' r='1' className='fill-yellow-400/20' />

      {/* Sight line (dotted). */}
      <line
        x1='50'
        y1='85'
        x2='47'
        y2='52'
        className='stroke-primary/10'
        strokeWidth='0.8'
        strokeDasharray='3 3'
      />

      {/* Rocket. */}
      <g transform='translate(130, 85)'>
        <path d='M-2 25 C-3 29, -1 32, 0 30 C1 32, 3 29, 2 25' className='fill-orange-400/60' />
        <path
          d='M0 -28 C-8 -14, -10 2, -10 18 L-9 24 H9 L10 18 C10 2, 8 -14, 0 -28Z'
          className='fill-primary'
        />
        <path
          d='M0 -26 C-5 -14, -6 2, -6 16 L-5.5 21 H-1 L-1.5 16 C-1.5 2, -2 -14, 0 -26Z'
          className='fill-white/18'
        />
        <circle cx='0' cy='-2' r='5.5' className='fill-primary-foreground/80' />
        <circle cx='0' cy='-2' r='4' className='fill-background/60' />
        <ellipse cx='-1' cy='-3.5' rx='1' ry='1.5' className='fill-white/25' />
        <path d='M-10 14 C-14 19, -16 26, -14 30 L-9 24 L-10 14Z' className='fill-primary' />
        <path d='M10 14 C14 19, 16 26, 14 30 L9 24 L10 14Z' className='fill-primary' />
        <rect x='-5.5' y='22' width='11' height='3.5' rx='1' className='fill-muted-foreground/35' />
      </g>

      {/* Stars. */}
      <circle cx='170' cy='35' r='1.5' className='fill-primary/25' />
      <circle cx='30' cy='80' r='1' className='fill-primary/15' />
      <circle cx='165' cy='145' r='1' className='fill-primary/18' />
      <circle cx='75' cy='165' r='1.5' className='fill-primary/15' />
      <circle cx='155' cy='60' r='1' className='fill-primary/20' />
    </svg>
  );
}
