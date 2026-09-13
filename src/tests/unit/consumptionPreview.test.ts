import { describe, expect, it } from 'vitest';
import type { MealFifoPreview, ProductFifoPreview } from '../../lib/types';
import {
  hasAnyConsumption,
  mealItemsFromForm,
  mealPreviewFromFifo,
  parseBeneficiaryCounts,
  productFifoCostsForMeal,
} from '../../components/consumption/preview';
import { MEAL_OPTIONS, type MealFormState } from '../../components/consumption/types';

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

  it('consumes backend meal_average directly rather than dividing cost by beneficiaries', () => {
    const backendMeal: MealFifoPreview = {
      meal_type: 'breakfast',
      predicted_fifo_cost: 100,
      total_beneficiaries: 4,
      meal_average: 25.5,
      product_previews: [],
    };

    const preview = mealPreviewFromFifo(backendMeal);

    expect(preview.totalBeneficiaries).toBe(4);
    expect(preview.totalCost).toBe(100);
    expect(preview.mealAverage).toBe(25.5);
  });

  it('consumes backend total_beneficiaries rather than summing the form', () => {
    const backendMeal: MealFifoPreview = {
      meal_type: 'lunch',
      predicted_fifo_cost: 60,
      total_beneficiaries: 3,
      meal_average: 20,
      product_previews: [],
    };

    const preview = mealPreviewFromFifo(backendMeal);

    expect(preview.totalBeneficiaries).toBe(3);
  });

  it('returns zeroed preview when no backend meal preview exists', () => {
    expect(mealPreviewFromFifo(null)).toEqual({
      totalBeneficiaries: 0,
      totalCost: 0,
      mealAverage: 0,
    });
    expect(mealPreviewFromFifo(undefined)).toEqual({
      totalBeneficiaries: 0,
      totalCost: 0,
      mealAverage: 0,
    });
  });

  it('consumes backend unit_cost rather than dividing predicted_fifo_cost by quantity', () => {
    const backendProduct: ProductFifoPreview = {
      product_id: 'p1',
      quantity: 3,
      predicted_fifo_cost: 60,
      unit_cost: 21.5,
      predicted_consumption_layers: [],
    };
    const backendMeal: MealFifoPreview = {
      meal_type: 'breakfast',
      predicted_fifo_cost: 60,
      total_beneficiaries: 3,
      meal_average: 20,
      product_previews: [backendProduct],
    };

    const costs = productFifoCostsForMeal(backendMeal);

    expect(costs['p1']).toEqual({ unitCost: 21.5, lineTotal: 60 });
  });

  it('returns no per-product costs when backend meal preview is absent', () => {
    expect(productFifoCostsForMeal(null)).toEqual({});
    expect(productFifoCostsForMeal(undefined)).toEqual({});
  });

  it('builds consumption items from user quantities', () => {
    const form: MealFormState = {
      beneficiaries: { staff24h: '', staff8h: '', reservation: '', mission: '', guest: '' },
      quantities: { p1: '2.5', p2: '', p3: '0' },
    };

    expect(mealItemsFromForm(form)).toEqual([{ product_id: 'p1', quantity: 2.5 }]);
  });

  it('detects consumption when any meal has activity', () => {
    const forms = Object.fromEntries(
      MEAL_OPTIONS.map((m) => [m.id, {
        beneficiaries: { staff24h: '0', staff8h: '0', reservation: '0', mission: '0', guest: '0' },
        quantities: {},
      }])
    ) as Record<typeof MEAL_OPTIONS[number]['id'], MealFormState>;

    expect(hasAnyConsumption(forms)).toBe(false);

    forms.lunch = {
      beneficiaries: { staff24h: '1', staff8h: '', reservation: '', mission: '', guest: '' },
      quantities: {},
    };
    expect(hasAnyConsumption(forms)).toBe(true);
  });
});
