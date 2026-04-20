// Rocket with wrench and gears — under construction.
// Best for: work in progress, maintenance, coming soon.

interface Props {
  className?: string;
}

export function RocketConstruction({ className = '' }: Props) {
  return (
    <svg
      viewBox='0 0 200 200'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={className}
      aria-hidden='true'
      focusable='false'
    >
      <circle cx='100' cy='100' r='75' className='fill-primary/5' />

      {/* Rocket body — slightly tilted. */}
      <g transform='translate(90, 100) rotate(-10)'>
        <path
          d='M0 -45 C-10 -25, -13 0, -12 25 L-11 42 H11 L12 25 C13 0, 10 -25, 0 -45Z'
          className='fill-primary'
        />
        <path
          d='M0 -43 C-6 -25, -8 0, -8 22 L-7 38 H-1 L-2 22 C-2 0, -3 -25, 0 -43Z'
          className='fill-white/15'
        />
        <circle cx='0' cy='-8' r='8' className='fill-primary-foreground/80' />
        <circle cx='0' cy='-8' r='6' className='fill-background/55' />
        <ellipse cx='-1.5' cy='-10' rx='1.5' ry='2' className='fill-white/25' />
        <path d='M-12 20 C-18 26, -21 35, -19 42 L-11 38 L-12 20Z' className='fill-primary' />
        <path d='M12 20 C18 26, 21 35, 19 42 L11 38 L12 20Z' className='fill-primary' />
        <rect x='-7' y='38' width='14' height='5' rx='1.5' className='fill-muted-foreground/35' />
      </g>

      {/* Wrench. */}
      <g transform='translate(140, 75) rotate(30)'>
        <rect x='-3' y='-20' width='6' height='40' rx='2' className='fill-muted-foreground/40' />
        <circle cx='0' cy='-22' r='8' className='stroke-muted-foreground/40' strokeWidth='4' />
        <circle cx='0' cy='-22' r='4' className='fill-background' />
        <path d='M-5 18 L-7 24 H7 L5 18Z' className='fill-muted-foreground/40' />
      </g>

      {/* Gear. */}
      <g transform='translate(50, 60)'>
        <circle cx='0' cy='0' r='12' className='fill-muted-foreground/20' />
        <circle cx='0' cy='0' r='5' className='fill-background' />
        {/* Gear teeth. */}
        {[0, 45, 90, 135, 180, 225, 270, 315].map((angle) => (
          <rect
            key={angle}
            x='-3'
            y='-15'
            width='6'
            height='6'
            rx='1'
            transform={`rotate(${angle})`}
            className='fill-muted-foreground/20'
          />
        ))}
      </g>

      {/* Small gear. */}
      <g transform='translate(60, 145)'>
        <circle cx='0' cy='0' r='8' className='fill-muted-foreground/15' />
        <circle cx='0' cy='0' r='3.5' className='fill-background' />
        {[0, 60, 120, 180, 240, 300].map((angle) => (
          <rect
            key={angle}
            x='-2.5'
            y='-10'
            width='5'
            height='4'
            rx='1'
            transform={`rotate(${angle})`}
            className='fill-muted-foreground/15'
          />
        ))}
      </g>

      {/* Stars. */}
      <circle cx='165' cy='40' r='1.5' className='fill-primary/25' />
      <circle cx='30' cy='35' r='1' className='fill-primary/20' />
      <circle cx='170' cy='155' r='1.5' className='fill-primary/15' />
    </svg>
  );
}
