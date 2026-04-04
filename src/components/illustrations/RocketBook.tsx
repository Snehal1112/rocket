// Rocket with an open book.
// Best for: documentation empty, help/guide states.

interface Props {
  className?: string;
}

export function RocketBook({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      <circle cx='100' cy='100' r='75' className='fill-primary/5' />

      {/* Open book. */}
      <g transform='translate(100, 140)'>
        {/* Left page. */}
        <path
          d='M-5 0 C-15 -5, -35 -8, -45 -5 L-45 -35 C-35 -38, -15 -35, -5 -30Z'
          className='fill-muted/70'
        />
        {/* Right page. */}
        <path
          d='M5 0 C15 -5, 35 -8, 45 -5 L45 -35 C35 -38, 15 -35, 5 -30Z'
          className='fill-muted/60'
        />
        {/* Spine. */}
        <path d='M-5 0 L-5 -30 L0 -33 L5 -30 L5 0 L0 -3Z' className='fill-muted-foreground/20' />
        {/* Page lines. */}
        <line
          x1='-15'
          y1='-12'
          x2='-38'
          y2='-15'
          className='stroke-muted-foreground/15'
          strokeWidth='1'
        />
        <line
          x1='-15'
          y1='-18'
          x2='-38'
          y2='-21'
          className='stroke-muted-foreground/15'
          strokeWidth='1'
        />
        <line
          x1='-15'
          y1='-24'
          x2='-38'
          y2='-27'
          className='stroke-muted-foreground/15'
          strokeWidth='1'
        />
        <line
          x1='15'
          y1='-12'
          x2='38'
          y2='-15'
          className='stroke-muted-foreground/12'
          strokeWidth='1'
        />
        <line
          x1='15'
          y1='-18'
          x2='38'
          y2='-21'
          className='stroke-muted-foreground/12'
          strokeWidth='1'
        />
        <line
          x1='15'
          y1='-24'
          x2='38'
          y2='-27'
          className='stroke-muted-foreground/12'
          strokeWidth='1'
        />
      </g>

      {/* Rocket rising from the book. */}
      <g transform='translate(100, 70)'>
        <path d='M-2 22 C-3 26, -1 30, 0 28 C1 30, 3 26, 2 22' className='fill-orange-400/60' />
        <path d='M0 -22 C-7 -10, -9 2, -9 16 H9 C9 2, 7 -10, 0 -22Z' className='fill-primary' />
        <path
          d='M0 -21 C-4 -10, -5.5 2, -5.5 14 H-1 C-1 2, -2 -10, 0 -21Z'
          className='fill-white/18'
        />
        <circle cx='0' cy='0' r='5' className='fill-primary-foreground/80' />
        <circle cx='0' cy='0' r='3.8' className='fill-background/60' />
        <ellipse cx='-1' cy='-1' rx='1' ry='1.5' className='fill-white/25' />
        <path d='M-9 12 L-13 19 L-9 17Z' className='fill-primary' />
        <path d='M9 12 L13 19 L9 17Z' className='fill-primary' />
        <rect x='-5' y='17' width='10' height='3.5' rx='1' className='fill-muted-foreground/35' />
      </g>

      {/* Sparkle particles from book. */}
      <circle cx='75' cy='110' r='2' className='fill-primary/25' />
      <circle cx='125' cy='108' r='1.5' className='fill-primary/20' />
      <circle cx='68' cy='95' r='1' className='fill-primary/15' />
      <circle cx='132' cy='92' r='1.5' className='fill-primary/18' />

      {/* Stars. */}
      <circle cx='35' cy='35' r='1.5' className='fill-primary/25' />
      <circle cx='165' cy='40' r='1' className='fill-primary/20' />
      <circle cx='30' cy='85' r='1' className='fill-primary/15' />
      <circle cx='170' cy='80' r='1.5' className='fill-primary/18' />
    </svg>
  );
}
