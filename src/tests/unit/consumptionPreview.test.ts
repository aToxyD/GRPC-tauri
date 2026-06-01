import { describe, expect, it } from 'vitest';
import {
  parseBeneficiaryCounts,
} from '../../components/consumption/preview';

describe('consumption preview', () => {
  it('parses beneficiary counts safely', () => {
    expect(parseBeneficiaryCounts({
      staff24h: '1',
      staff8h: '',
      reservation: '2',
      mission: 'x',
      guest: '3',
    })).toEqual({
      staff_24h_count: 1,
      staff_8h_count: 0,
      reservation_count: 2,
      mission_count: 0,
      guest_count: 3,
    });
  });
});
