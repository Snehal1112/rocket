import { ArrowRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { Party } from '@/types/contracts';
import { PartyPill } from './PartyPill';

const MAX_VISIBLE = 5;

/** Rendered height of a PartyPill in px. */
const ROW_H = 28;
/** gap-1.5 = 6px between rows. */
const GAP = 6;
const STRIDE = ROW_H + GAP;
/** Base SVG connector width; grows with row count so curves don't feel cramped. */
const SVG_W_BASE = 48;
const SVG_W_PER_ROW = 10;

interface ContractPartiesProps {
  provider: Party;
  consumers: Party[];
  /** Override the default visible cap of 5. */
  maxVisible?: number;
}

/**
 * Renders the provider-to-consumers section of a contract card.
 *
 * Single consumer: inline [Provider] → [Consumer].
 * Multiple consumers: provider vertically centered against the consumer stack,
 * connected by smooth SVG bezier curves branching to each consumer pill.
 */
export function ContractParties({
  provider,
  consumers,
  maxVisible = MAX_VISIBLE,
}: ContractPartiesProps) {
  const visible = consumers.slice(0, maxVisible);
  const overflow = consumers.slice(maxVisible);
  const hasOverflow = overflow.length > 0;
  const rows = hasOverflow ? [...visible, null] : visible;

  if (consumers.length <= 1) {
    return (
      <div className='flex items-center gap-2 flex-wrap'>
        <PartyPill party={provider} partyRole='provider' />
        <ArrowRight className='w-4 h-3.5 text-muted-foreground shrink-0' aria-hidden='true' />
        {consumers[0] && <PartyPill party={consumers[0]} partyRole='consumer' />}
      </div>
    );
  }

  const totalH = rows.length * STRIDE - GAP;
  const originY = totalH / 2;
  // Offset the provider pill so its vertical center aligns with originY.
  const providerOffset = Math.max(0, originY - ROW_H / 2);
  // Wider SVG for more rows so curves have room to fan out without looking cramped.
  const svgW = SVG_W_BASE + Math.max(0, rows.length - 2) * SVG_W_PER_ROW;

  return (
    <TooltipProvider delayDuration={200}>
      <div className='flex items-start gap-0'>
        {/* Provider pill — vertically centered against the consumer stack */}
        <div style={{ paddingTop: providerOffset }}>
          <PartyPill party={provider} partyRole='provider' />
        </div>

        {/* SVG bezier curves from provider right-center to each consumer left-center */}
        <svg
          aria-hidden='true'
          width={svgW}
          height={totalH}
          viewBox={`0 0 ${svgW} ${totalH}`}
          className='shrink-0 self-start'
        >
          {rows.map((consumer, i) => {
            const targetY = i * STRIDE + ROW_H / 2;
            return (
              <path
                key={consumer ? consumer.id : '__overflow'}
                d={`M 0 ${originY} C ${svgW * 0.55} ${originY}, ${svgW * 0.45} ${targetY}, ${svgW} ${targetY}`}
                fill='none'
                stroke='hsl(var(--border))'
                strokeWidth='1.5'
                strokeLinecap='round'
              />
            );
          })}
        </svg>

        {/* Consumer pill rows */}
        <div className='flex flex-col gap-1.5'>
          {rows.map((consumer) => (
            <div key={consumer ? consumer.id : '__overflow'} className='flex items-center'>
              {consumer === null ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='outline'
                      size='sm'
                      onClick={(e) => e.stopPropagation()}
                      className='h-auto rounded-full px-[10px] py-1 text-xs text-muted-foreground font-normal'
                      aria-label={`${overflow.length} more consumers`}
                    >
                      +{overflow.length} more
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side='right'>
                    {overflow.map((c) => (
                      <span key={c.id} className='block'>
                        {c.name}
                      </span>
                    ))}
                  </TooltipContent>
                </Tooltip>
              ) : (
                <PartyPill party={consumer} partyRole='consumer' />
              )}
            </div>
          ))}
        </div>
      </div>
    </TooltipProvider>
  );
}
