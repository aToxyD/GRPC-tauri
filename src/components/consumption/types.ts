import type { MealType, Product } from '../../lib/types';

export type BeneficiaryFields = {
  staff24h: string;
  staff8h: string;
  reservation: string;
  mission: string;
  guest: string;
};

export type MealFormState = {
  beneficiaries: BeneficiaryFields;
  quantities: Record<string, string>;
};

export type ConsumptionProductRow = {
  product: Product;
  available: boolean;
  stock: number;
};

export type MealPreview = {
  totalBeneficiaries: number;
  totalCost: number;
  mealAverage: number;
};

export const MEAL_OPTIONS: { id: MealType; label: string }[] = [
  { id: 'breakfast', label: 'الفطور' },
  { id: 'lunch', label: 'الغداء' },
  { id: 'dinner', label: 'العشاء' },
];

export function emptyBeneficiaries(): BeneficiaryFields {
  return { staff24h: '', staff8h: '', reservation: '', mission: '', guest: '' };
}

export function emptyMealState(): MealFormState {
  return { beneficiaries: emptyBeneficiaries(), quantities: {} };
}

export function emptyMealForms(): Record<MealType, MealFormState> {
  return {
    breakfast: emptyMealState(),
    lunch: emptyMealState(),
    dinner: emptyMealState(),
  };
}
