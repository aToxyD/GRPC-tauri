import type {
  ConsumptionItemInput,
  DailyConsumptionSummary,
  DailyConsumptionView,
  MealType,
  Product,
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

export function computeMealPreview(form: MealFormState, products: Product[]): MealPreview {
  const counts = parseBeneficiaryCounts(form.beneficiaries);
  const totalBeneficiaries =
    counts.staff_24h_count +
    counts.staff_8h_count +
    counts.reservation_count +
    counts.mission_count +
    counts.guest_count;

  let totalCost = 0;
  for (const item of mealItemsFromForm(form)) {
    const product = products.find((p) => p.id === item.product_id);
    if (product) totalCost += item.quantity * product.base_price;
  }

  const mealAverage = totalBeneficiaries > 0 ? totalCost / totalBeneficiaries : 0;
  return { totalBeneficiaries, totalCost, mealAverage };
}

export function computeDailySummary(
  mealForms: Record<MealType, MealFormState>,
  products: Product[]
): DailyConsumptionSummary {
  const b = computeMealPreview(mealForms.breakfast, products);
  const l = computeMealPreview(mealForms.lunch, products);
  const d = computeMealPreview(mealForms.dinner, products);

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
  const counts = parseBeneficiaryCounts(form.beneficiaries);
  const total =
    counts.staff_24h_count +
    counts.staff_8h_count +
    counts.reservation_count +
    counts.mission_count +
    counts.guest_count;
  return mealItemsFromForm(form).length > 0 || total > 0;
}

export function hasAnyConsumption(mealForms: Record<MealType, MealFormState>): boolean {
  return MEAL_OPTIONS.some((meal) => mealHasActivity(mealForms[meal.id]));
}
