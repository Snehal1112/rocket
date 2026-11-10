import { describe, expect, it } from 'vitest';
import {
  contractCheckToastId,
  NO_ANSWER_TEXT,
  offerToastText,
  resultStateLabel,
  verdictLabel,
} from '@/lib/network-contract-check-utils';

describe('contract check utils', () => {
  it('labels verdicts', () => {
    expect(verdictLabel('pass')).toBe('Pass');
    expect(verdictLabel('break')).toBe('Break');
    expect(verdictLabel('not_used')).toBe('Not used');
  });

  it('says No answer yet without an entry', () => {
    expect(resultStateLabel(undefined)).toBe(NO_ANSWER_TEXT);
    expect(resultStateLabel('busy')).toBe('Busy');
    expect(resultStateLabel('expired')).toBe('Expired');
  });

  it('builds the offer text for one and many changes', () => {
    expect(offerToastText('Ben', 'Orders API', 1)).toBe(
      '"Ben" wants to check a change to "Orders API" (1 change)',
    );
    expect(offerToastText(null, 'Orders API', 3)).toBe(
      '"A teammate" wants to check a change to "Orders API" (3 changes)',
    );
  });

  it('keeps one toast id per check', () => {
    expect(contractCheckToastId('c1')).not.toBe(contractCheckToastId('c2'));
  });
});
