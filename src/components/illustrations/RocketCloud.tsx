// Rocket flying above clouds.
// Best for: upload, deploy, cloud sync states.

interface Props {
  className?: string;
}

export function RocketCloud({ className = '' }: Props) {
  return (
    <svg viewBox='0 0 200 200' fill='none' xmlns='http://www.w3.org/2000/svg' className={className}>
      {/* Sky glow. */}
      <circle cx='100' cy='80' r='70' className='fill-primary/5' />

      {/* Clouds. */}
      <g className='fill-muted/60'>
        <ellipse cx='55' cy='150' rx='30' ry='12' />
        <ellipse cx='40' cy='145' rx='15' ry='10' />
        <ellipse cx='75' cy='146' rx='18' ry='9' />
      </g>
      <g className='fill-muted/50'>
        <ellipse cx='145' cy='155' rx='25' ry='10' />
        <ellipse cx='130' cy='150' rx='15' ry='9' />
        <ellipse cx='160' cy='151' rx='14' ry='8' />
      </g>
      <g className='fill-muted/40'>
        <ellipse cx='100' cy='165' rx='35' ry='11' />
        <ellipse cx='80' cy='161' rx='18' ry='9' />
        <ellipse cx='120' cy='162' rx='16' ry='8' />
      </g>

      {/* Rocket flying upward. */}
      <g transform='translate(100, 75)'>
        {/* Flame. */}
        <path d='M-6 42 C-8 50, -4 58, 0 54 C4 58, 8 50, 6 42' className='fill-orange-400/70' />
        <path d='M-3 42 C-4 47, -1 52, 0 50 C1 52, 4 47, 3 42' className='fill-yellow-400/80' />

        {/* Body. */}
        <path
          d='M0 -40 C-10 -22, -13 2, -12 22 L-11 38 H11 L12 22 C13 2, 10 -22, 0 -40Z'
          className='fill-primary'
        />
        <path
          d='M0 -38 C-6 -22, -8 2, -8 20 L-7 34 H-1 L-2 20 C-2 2, -3 -22, 0 -38Z'
          className='fill-white/20'
        />

        {/* Window. */}
        <circle cx='0' cy='-5' r='7' className='fill-primary-foreground/85' />
        <circle cx='0' cy='-5' r='5.5' className='fill-background/60' />
        <ellipse cx='-1.5' cy='-7' rx='1.5' ry='2' className='fill-white/25' />

        {/* Fins. */}
        <path d='M-12 18 C-18 24, -20 33, -18 38 L-11 34 L-12 18Z' className='fill-primary' />
        <path d='M12 18 C18 24, 20 33, 18 38 L11 34 L12 18Z' className='fill-primary' />

        {/* Nozzle. */}
        <rect x='-7' y='34' width='14' height='5' rx='1.5' className='fill-muted-foreground/35' />
      </g>

      {/* Speed lines. */}
      <line
        x1='85'
        y1='120'
        x2='85'
        y2='132'
        className='stroke-muted-foreground/15'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='115'
        y1='118'
        x2='115'
        y2='130'
        className='stroke-muted-foreground/15'
        strokeWidth='1.5'
        strokeLinecap='round'
      />
      <line
        x1='100'
        y1='125'
        x2='100'
        y2='140'
        className='stroke-muted-foreground/12'
        strokeWidth='1'
        strokeLinecap='round'
      />

      {/* Stars. */}
      <circle cx='35' cy='30' r='1.5' className='fill-primary/30' />
      <circle cx='165' cy='25' r='1' className='fill-primary/20' />
      <circle cx='170' cy='70' r='1.5' className='fill-primary/25' />
      <circle cx='30' cy='75' r='1' className='fill-primary/15' />
    </svg>
  );
}
