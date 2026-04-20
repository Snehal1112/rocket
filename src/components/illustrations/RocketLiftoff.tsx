// Rocket with a magnifying glass — searching through space.
// Best for: "no results found", search empty states, history empty.

interface Props {
  className?: string;
}

export function RocketLiftOff({ className = '' }: Props) {
  return (
    <svg
      width='120'
      height='120'
      viewBox='0 0 120 120'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      className={`mx-auto opacity-80 drop-shadow-lg ${className}`}
      aria-hidden='true'
      focusable='false'
    >
      {/* Exhaust glow */}
      <ellipse cx='60' cy='108' rx='12' ry='4' className='fill-primary/15' />
      <ellipse cx='60' cy='104' rx='8' ry='3' className='fill-primary/25' />
      {/* Rocket body */}
      <path
        d='M60 20 C60 20, 78 40, 78 70 C78 85, 70 95, 60 95 C50 95, 42 85, 42 70 C42 40, 60 20, 60 20Z'
        className='fill-muted-foreground/20 stroke-muted-foreground/40'
        strokeWidth='1.5'
      />
      {/* Nose cone */}
      <path
        d='M60 20 C60 20, 68 35, 68 45 C68 45, 60 42, 52 45 C52 35, 60 20, 60 20Z'
        className='fill-primary/80'
      />
      {/* Window */}
      <circle
        cx='60'
        cy='55'
        r='8'
        className='fill-background stroke-muted-foreground/40'
        strokeWidth='1.5'
      />
      <circle cx='60' cy='55' r='5' className='fill-background' />
      <circle cx='57' cy='52' r='2' className='fill-primary/40' />
      {/* Fins */}
      <path d='M42 75 L30 90 L42 88Z' className='fill-primary/50' />
      <path d='M78 75 L90 90 L78 88Z' className='fill-primary/50' />
      {/* Flame */}
      <path d='M54 95 Q60 115 66 95' className='fill-amber-500/60' />
      <path d='M56 95 Q60 108 64 95' className='fill-red-500/40' />
    </svg>
  );
}
