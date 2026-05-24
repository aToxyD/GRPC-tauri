import { describe, expect, it } from 'vitest';
import {
  computeDailySummary,
  computeMealPreview,
  dailySummaryFromFormsAndFifo,
  mealPreviewFromFifo,
  parseBeneficiaryCounts,
} from '../../components/consumption/preview';
import { emptyMealForms, emptyMealState } from '../../components/consumption/types';
import type { DailyFifoConsumptionPreview } from '../../lib/types';

describe('consumption preview', () => {
  it('computes meal beneficiaries without catalog pricing', () => {
    const form = emptyMealState();
    form.beneficiaries.staff24h = '5';
    form.beneficiaries.guest = '5';
    form.quantities.p1 = '2';

    const preview = computeMealPreview(form);
    expect(preview.totalBeneficiaries).toBe(10);
    expect(preview.totalCost).toBe(0);
    expect(preview.mealAverage).toBe(0);
  });

  it('uses FIFO meal preview costs when provided', () => {
    const form = emptyMealState();
    form.beneficiaries.staff24h = '10';
    form.quantities.p1 = '2';

    const preview = mealPreviewFromFifo(form, {
      meal_type: 'breakfast',
      predicted_fifo_cost: 100,
      product_previews: [],
    });
    expect(preview.totalCost).toBe(100);
    expect(preview.mealAverage).toBe(10);
  });

  it('merges FIFO preview with beneficiary counts for daily summary', () => {
    const forms = emptyMealForms();
    forms.breakfast.beneficiaries.staff24h = '10';
    forms.lunch.beneficiaries.staff24h = '20';
    forms.dinner.beneficiaries.staff24h = '30';

    const fifoPreview: DailyFifoConsumptionPreview = {
      predicted_fifo_cost: 600,
      predicted_consumption_layers: [],
      predicted_remaining_inventory_value: 0,
      meal_previews: [
        { meal_type: 'breakfast', predicted_fifo_cost: 100, product_previews: [] },
        { meal_type: 'lunch', predicted_fifo_cost: 200, product_previews: [] },
        { meal_type: 'dinner', predicted_fifo_cost: 300, product_previews: [] },
      ],
    };

    const summary = dailySummaryFromFormsAndFifo(forms, fifoPreview);
    expect(summary.total_daily_beneficiaries).toBe(60);
    expect(summary.total_daily_cost).toBe(600);
    expect(summary.breakfast_cost).toBe(100);
    expect(summary.daily_average).toBe(10 + 10 + 10);
  });

  it('sums beneficiary-only daily totals without catalog prices', () => {
    const forms = emptyMealForms();
    forms.breakfast.beneficiaries.staff24h = '10';
    forms.lunch.beneficiaries.staff24h = '20';

    const summary = computeDailySummary(forms);
    expect(summary.total_daily_beneficiaries).toBe(30);
    expect(summary.total_daily_cost).toBe(0);
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
