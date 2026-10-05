import { describe, expect, it } from 'vitest';
import { base64ToBytes, isPreviewableImage, suggestedFileName } from '@/lib/response-binary';

describe('response-binary', () => {
  it('decodes base64 to the exact bytes', () => {
    expect(Array.from(base64ToBytes('iVBORw=='))).toEqual([0x89, 0x50, 0x4e, 0x47]);
    expect(base64ToBytes('').length).toBe(0);
  });

  it('previews common image types only', () => {
    for (const ct of ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/svg+xml']) {
      expect(isPreviewableImage(ct)).toBe(true);
    }
    expect(isPreviewableImage('application/pdf')).toBe(false);
    expect(isPreviewableImage('image/tiff')).toBe(false);
  });

  it('takes the file name from Content-Disposition and strips any directory part', () => {
    const headers = [
      { key: 'Content-Disposition', value: 'attachment; filename="../../etc/report.pdf"' },
    ];
    expect(suggestedFileName(headers, 'application/pdf')).toBe('report.pdf');
    const star = [{ key: 'content-disposition', value: "attachment; filename*=UTF-8''a%20b.zip" }];
    expect(suggestedFileName(star, 'application/zip')).toBe('a b.zip');
  });

  it('falls back to response plus an extension from the content type', () => {
    expect(suggestedFileName([], 'image/png')).toBe('response.png');
    expect(suggestedFileName([], 'application/pdf')).toBe('response.pdf');
    expect(suggestedFileName([], 'application/x-unknown')).toBe('response.bin');
  });
});
