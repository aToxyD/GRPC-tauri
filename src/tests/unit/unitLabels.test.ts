import { describe, expect, it } from 'vitest';
import { UNIT_CODES, UNIT_LABELS, unitLabel } from '../../lib/unitLabels';

describe('unitLabels', () => {
  it('maps all ten authoritative unit codes to the canonical labels', () => {
    expect(UNIT_LABELS).toEqual({
      1: 'كلغ',
      2: 'لتر',
      3: 'دلو',
      4: 'قارورة',
      5: 'صفيحة',
      6: 'قطعة',
      7: 'بيضة',
      8: 'علبة',
      9: 'كيس',
      10: 'خبزة',
    });
  });

  it('exposes the closed 1..=10 code set for UI iteration', () => {
    expect(UNIT_CODES).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  });

  it('resolves known codes to their labels', () => {
    expect(unitLabel(1)).toBe('كلغ');
    expect(unitLabel(6)).toBe('قطعة');
    expect(unitLabel(10)).toBe('خبزة');
  });

  it('falls back to the numeric code for unknown codes', () => {
    expect(unitLabel(0)).toBe('0');
    expect(unitLabel(11)).toBe('11');
    expect(unitLabel(42)).toBe('42');
  });
});