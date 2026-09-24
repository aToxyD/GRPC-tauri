import type { ConsumptionItemInput, MealFifoPreview, MealType } from '../../lib/types';
import type { BeneficiaryFields, MealFormState, MealPreview } from './types';
import { MEAL_OPTIONS } from './types';

export function formatAmount(value: number): string {
  return `${value.toFixed(2)} دج`;
}

export function parseBeneficiaryCounts(b: BeneficiaryFields) {
  return {
    staff_24h_count: parseInt(b.staff24h, 10) || 0,
    staff_8h_count: parseInt(b.staff8h, 10) || 0,
    reservation_count: parseInt(b.reservation, 10) || 0,
    mission_count: parseInt(b.mission, 10) || 0,
    guest_count: parseInt(b.guest, 10) || 0,
  };
}

export function mealItemsFromForm(form: MealFormState): ConsumptionItemInput[] {
  return Object.entries(form.quantities)
    .filter(([, qty]) => qty && parseFloat(qty) > 0)
    .map(([product_id, qty]) => ({
      product_id,
      quantity: parseFloat(qty),
    }));
}

export function totalBeneficiariesFromForm(form: MealFormState): number {
  const counts = parseBeneficiaryCounts(form.beneficiaries);
  return (
    counts.staff_24h_count +
    counts.staff_8h_count +
    counts.reservation_count +
    counts.mission_count +
    counts.guest_count
  );
}

export function mealPreviewFromFifo(fifoMeal?: MealFifoPreview | null): MealPreview {
  if (!fifoMeal) return { totalBeneficiaries: 0, totalCost: 0, mealAverage: 0 };
  return {
    totalBeneficiaries: fifoMeal.total_beneficiaries,
    totalCost: fifoMeal.predicted_fifo_cost,
    mealAverage: fifoMeal.meal_average,
  };
}

export function productFifoCostsForMeal(
  fifoMeal?: MealFifoPreview | null
): Record<string, { unitCost: number; lineTotal: number }> {
  const map: Record<string, { unitCost: number; lineTotal: number }> = {};
  if (!fifoMeal) return map;
  for (const p of fifoMeal.product_previews) {
    map[p.product_id] = { unitCost: p.unit_cost, lineTotal: p.predicted_fifo_cost };
  }
  return map;
}

export function mealHasActivity(form: MealFormState): boolean {
  const total = totalBeneficiariesFromForm(form);
  return mealItemsFromForm(form).length > 0 || total > 0;
}

export function hasAnyConsumption(mealForms: Record<MealType, MealFormState>): boolean {
  return MEAL_OPTIONS.some((meal) => mealHasActivity(mealForms[meal.id]));
}
