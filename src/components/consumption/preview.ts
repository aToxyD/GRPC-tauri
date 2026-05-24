import type {
  ConsumptionItemInput,
  DailyConsumptionSummary,
  DailyConsumptionView,
  DailyFifoConsumptionPreview,
  MealFifoPreview,
  MealType,
} from '../../lib/types';
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

export function mealPreviewFromFifo(form: MealFormState, fifoMeal?: MealFifoPreview | null): MealPreview {
  const totalBeneficiaries = totalBeneficiariesFromForm(form);
  const totalCost = fifoMeal?.predicted_fifo_cost ?? 0;
  const mealAverage = totalBeneficiaries > 0 ? totalCost / totalBeneficiaries : 0;
  return { totalBeneficiaries, totalCost, mealAverage };
}

export function productFifoCostsForMeal(
  fifoMeal?: MealFifoPreview | null
): Record<string, { unitCost: number; lineTotal: number }> {
  const map: Record<string, { unitCost: number; lineTotal: number }> = {};
  if (!fifoMeal) return map;
  for (const p of fifoMeal.product_previews) {
    const unitCost = p.quantity > 0 ? p.predicted_fifo_cost / p.quantity : 0;
    map[p.product_id] = { unitCost, lineTotal: p.predicted_fifo_cost };
  }
  return map;
}

export function summaryFromFifoPreview(preview: DailyFifoConsumptionPreview): DailyConsumptionSummary {
  const mealByType = new Map(preview.meal_previews.map((m) => [m.meal_type, m]));

  const breakfast = mealByType.get('breakfast');
  const lunch = mealByType.get('lunch');
  const dinner = mealByType.get('dinner');

  const breakfastBeneficiaries = 0;
  const lunchBeneficiaries = 0;
  const dinnerBeneficiaries = 0;

  return {
    breakfast_beneficiaries: breakfastBeneficiaries,
    lunch_beneficiaries: lunchBeneficiaries,
    dinner_beneficiaries: dinnerBeneficiaries,
    breakfast_cost: breakfast?.predicted_fifo_cost ?? 0,
    lunch_cost: lunch?.predicted_fifo_cost ?? 0,
    dinner_cost: dinner?.predicted_fifo_cost ?? 0,
    breakfast_average: 0,
    lunch_average: 0,
    dinner_average: 0,
    total_daily_beneficiaries: 0,
    total_daily_cost: preview.predicted_fifo_cost,
    daily_average: 0,
  };
}

/** Merge FIFO costs with beneficiary counts from meal forms. */
export function dailySummaryFromFormsAndFifo(
  mealForms: Record<MealType, MealFormState>,
  preview: DailyFifoConsumptionPreview
): DailyConsumptionSummary {
  const mealByType = new Map(preview.meal_previews.map((m) => [m.meal_type, m]));

  const bForm = mealForms.breakfast;
  const lForm = mealForms.lunch;
  const dForm = mealForms.dinner;

  const bBen = totalBeneficiariesFromForm(bForm);
  const lBen = totalBeneficiariesFromForm(lForm);
  const dBen = totalBeneficiariesFromForm(dForm);

  const bCost = mealByType.get('breakfast')?.predicted_fifo_cost ?? 0;
  const lCost = mealByType.get('lunch')?.predicted_fifo_cost ?? 0;
  const dCost = mealByType.get('dinner')?.predicted_fifo_cost ?? 0;

  const bAvg = bBen > 0 ? bCost / bBen : 0;
  const lAvg = lBen > 0 ? lCost / lBen : 0;
  const dAvg = dBen > 0 ? dCost / dBen : 0;

  return {
    breakfast_beneficiaries: bBen,
    lunch_beneficiaries: lBen,
    dinner_beneficiaries: dBen,
    breakfast_cost: bCost,
    lunch_cost: lCost,
    dinner_cost: dCost,
    breakfast_average: bAvg,
    lunch_average: lAvg,
    dinner_average: dAvg,
    total_daily_beneficiaries: bBen + lBen + dBen,
    total_daily_cost: preview.predicted_fifo_cost,
    daily_average: bAvg + lAvg + dAvg,
  };
}

export function summaryFromSaved(view: DailyConsumptionView): DailyConsumptionSummary {
  const s: DailyConsumptionSummary = {
    breakfast_beneficiaries: 0,
    lunch_beneficiaries: 0,
    dinner_beneficiaries: 0,
    breakfast_cost: 0,
    lunch_cost: 0,
    dinner_cost: 0,
    breakfast_average: 0,
    lunch_average: 0,
    dinner_average: 0,
    total_daily_beneficiaries: view.report.total_daily_beneficiaries,
    total_daily_cost: view.report.total_daily_cost,
    daily_average: view.report.total_daily_average,
  };

  for (const entry of view.meals) {
    const m = entry.meal;
    if (m.meal_type === 'breakfast') {
      s.breakfast_beneficiaries = m.total_beneficiaries;
      s.breakfast_cost = m.total_meal_cost;
      s.breakfast_average = m.meal_average;
    } else if (m.meal_type === 'lunch') {
      s.lunch_beneficiaries = m.total_beneficiaries;
      s.lunch_cost = m.total_meal_cost;
      s.lunch_average = m.meal_average;
    } else {
      s.dinner_beneficiaries = m.total_beneficiaries;
      s.dinner_cost = m.total_meal_cost;
      s.dinner_average = m.meal_average;
    }
  }

  return s;
}

export function mealHasActivity(form: MealFormState): boolean {
  const total = totalBeneficiariesFromForm(form);
  return mealItemsFromForm(form).length > 0 || total > 0;
}

export function hasAnyConsumption(mealForms: Record<MealType, MealFormState>): boolean {
  return MEAL_OPTIONS.some((meal) => mealHasActivity(mealForms[meal.id]));
}

/** @deprecated Use backend FIFO preview — kept for tests of beneficiary aggregation only. */
export function computeMealPreview(form: MealFormState): MealPreview {
  const totalBeneficiaries = totalBeneficiariesFromForm(form);
  return { totalBeneficiaries, totalCost: 0, mealAverage: 0 };
}

/** @deprecated Use dailySummaryFromFormsAndFifo */
export function computeDailySummary(mealForms: Record<MealType, MealFormState>): DailyConsumptionSummary {
  const b = computeMealPreview(mealForms.breakfast);
  const l = computeMealPreview(mealForms.lunch);
  const d = computeMealPreview(mealForms.dinner);

  return {
    breakfast_beneficiaries: b.totalBeneficiaries,
    lunch_beneficiaries: l.totalBeneficiaries,
    dinner_beneficiaries: d.totalBeneficiaries,
    breakfast_cost: b.totalCost,
    lunch_cost: l.totalCost,
    dinner_cost: d.totalCost,
    breakfast_average: b.mealAverage,
    lunch_average: l.mealAverage,
    dinner_average: d.mealAverage,
    total_daily_beneficiaries: b.totalBeneficiaries + l.totalBeneficiaries + d.totalBeneficiaries,
    total_daily_cost: b.totalCost + l.totalCost + d.totalCost,
    daily_average: b.mealAverage + l.mealAverage + d.mealAverage,
  };
}
