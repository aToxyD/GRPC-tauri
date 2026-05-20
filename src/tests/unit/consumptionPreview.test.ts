import { describe, expect, it } from 'vitest';
import {
  computeDailySummary,
  computeMealPreview,
  parseBeneficiaryCounts,
} from '../../components/consumption/preview';
import { emptyMealForms, emptyMealState } from '../../components/consumption/types';
import type { Product } from '../../lib/types';

const products: Product[] = [
  {
    id: 'p1',
    name: 'Bread',
    base_price: 10,
    tva: 0,
    supplier_name: null,
    year: 2026,
    created_at: '2026-01-01',
  },
];

describe('consumption preview', () => {
  it('computes meal beneficiaries and average from inputs', () => {
    const form = emptyMealState();
    form.beneficiaries.staff24h = '5';
    form.beneficiaries.guest = '5';
    form.quantities.p1 = '2';

    const preview = computeMealPreview(form, products);
    expect(preview.totalBeneficiaries).toBe(10);
    expect(preview.totalCost).toBe(20);
    expect(preview.mealAverage).toBe(2);
  });

  it('sums meal previews into daily totals without dividing by 3', () => {
    const forms = emptyMealForms();
    forms.breakfast.beneficiaries.staff24h = '10';
    forms.breakfast.quantities.p1 = '1';
    forms.lunch.beneficiaries.staff24h = '20';
    forms.lunch.quantities.p1 = '2';
    forms.dinner.beneficiaries.staff24h = '30';
    forms.dinner.quantities.p1 = '3';

    const summary = computeDailySummary(forms, products);
    expect(summary.total_daily_beneficiaries).toBe(60);
    expect(summary.total_daily_cost).toBe(60);
    expect(summary.daily_average).toBe(1 + 1 + 1);
  });

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
