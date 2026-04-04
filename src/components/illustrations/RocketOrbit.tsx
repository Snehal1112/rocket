// Rocket orbiting a planet with an orbital ring.
// Best for: loading states, "no results" screens, exploration prompts.

interface Props {
  className?: string;
}

export function RocketOrbit({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      {/* Planet. */}
      <circle cx='100' cy='110' r='40' className='fill-muted/80' />
      <ellipse cx='90' cy='105' rx='20' ry='25' className='fill-muted-foreground/8' />
      {/* Planet surface detail. */}
      <circle cx='85' cy='100' r='8' className='fill-muted-foreground/6' />
      <circle cx='110' cy='120' r='5' className='fill-muted-foreground/6' />
      <circle cx='105' cy='95' r='3' className='fill-muted-foreground/5' />

      {/* Orbital ring (behind rocket). */}
      <ellipse
        cx='100'
        cy='110'
        rx='65'
        ry='18'
        className='stroke-primary/20'
        strokeWidth='1.5'
        strokeDasharray='4 3'
        transform='rotate(-15, 100, 110)'
      />

      {/* Mini rocket on orbit — tilted 45 degrees, upper right. */}
      <g transform='translate(148, 80) rotate(45)'>
        {/* Flame. */}
        <path d='M-2 16 C-3 20, -1 24, 0 22 C1 24, 3 20, 2 16' className='fill-orange-400/70' />
        <path d='M-1 16 C-1.5 19, 0 21, 0 20 C0 21, 1.5 19, 1 16' className='fill-yellow-400/80' />

        {/* Body. */}
        <path d='M0 -12 C-4 -4, -5 4, -5 12 H5 C5 4, 4 -4, 0 -12Z' className='fill-primary' />
        <path d='M0 -12 C-2 -4, -3 4, -3 10 H-1 C-1 4, -1 -4, 0 -12Z' className='fill-white/20' />

        {/* Window. */}
        <circle cx='0' cy='0' r='3' className='fill-background/60' />
        <ellipse cx='-0.5' cy='-0.8' rx='1' ry='1.2' className='fill-white/30' />

        {/* Left fin. */}
        <path d='M-5 8 L-9 14 L-5 12Z' className='fill-primary' />
        {/* Right fin. */}
        <path d='M5 8 L9 14 L5 12Z' className='fill-primary' />

        {/* Nozzle. */}
        <rect x='-3' y='12' width='6' height='3' rx='1' className='fill-muted-foreground/40' />
      </g>

      {/* Orbital ring (in front of rocket). */}
      <path
        d='M45 105 Q70 98, 100 92'
        className='stroke-primary/20'
        strokeWidth='1.5'
        strokeDasharray='4 3'
        fill='none'
      />

      {/* Stars scattered. */}
      <circle cx='30' cy='40' r='2' className='fill-primary/35' />
      <circle cx='170' cy='35' r='1.5' className='fill-primary/25' />
      <circle cx='20' cy='90' r='1' className='fill-primary/20' />
      <circle cx='175' cy='140' r='1.5' className='fill-primary/30' />
      <circle cx='25' cy='160' r='1' className='fill-primary/15' />
      <circle cx='160' cy='170' r='2' className='fill-primary/20' />
      <circle cx='55' cy='25' r='1.5' className='fill-primary/25' />
      <circle cx='145' cy='25' r='1' className='fill-primary/20' />

      {/* Twinkle stars (cross shape). */}
      <g className='fill-primary/30'>
        <path d='M40 70 L41 67 L42 70 L41 73Z' />
        <path d='M40 70 L37 71 L40 72 L43 71Z' />
      </g>
      <g className='fill-primary/25'>
        <path d='M165 60 L166 58 L167 60 L166 62Z' />
        <path d='M165 60 L163 61 L165 62 L167 61Z' />
      </g>
    </svg>
  );
}
