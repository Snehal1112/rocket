import { cva } from 'class-variance-authority';

/**
 * Shared button styling. It lives apart from the component so anchors can wear
 * the same look without importing a component they do not render.
 */
export const buttonVariants = cva(
  [
    'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-lg',
    'font-heading font-medium tracking-tight',
    'transition-[background-color,border-color,color,box-shadow,transform] duration-200 ease-out',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    'focus-visible:ring-offset-2 focus-visible:ring-offset-background',
    'disabled:pointer-events-none disabled:opacity-50',
    '[&_svg]:shrink-0',
  ].join(' '),
  {
    variants: {
      variant: {
        primary:
          'bg-primary text-primary-foreground shadow-sm hover:bg-primary/90 hover:shadow-md active:translate-y-px',
        outline:
          'border border-border bg-card text-foreground hover:border-primary/50 hover:bg-accent active:translate-y-px',
        ghost: 'text-muted-foreground hover:bg-accent hover:text-foreground',
        inverse:
          'bg-primary-foreground text-primary shadow-sm hover:bg-primary-foreground/90 active:translate-y-px',
      },
      size: {
        sm: 'h-9 px-3.5 text-sm [&_svg]:size-4',
        md: 'h-10 px-4 text-sm [&_svg]:size-4',
        lg: 'h-12 px-6 text-base [&_svg]:size-[18px]',
        icon: 'size-9 [&_svg]:size-[18px]',
      },
    },
    defaultVariants: { variant: 'primary', size: 'md' },
  },
);
