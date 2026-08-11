import { safeInvoke } from '../tauri';
import type {
  DailyReportResult, DailyReportInput, DailyConsumptionView,
  DailyFifoConsumptionPreview, DailyReport, MealConsumptionInput,
} from '../types';

export async function createDailyReport(input: DailyReportInput, unitId?: string): Promise<DailyReportResult> {
  return await safeInvoke('create_daily_report', { input, unitId });
}

export async function previewDailyConsumptionFifo(input: DailyReportInput): Promise<DailyFifoConsumptionPreview> {
  return await safeInvoke('preview_daily_consumption_fifo', { input });
}

export async function createMealConsumption(input: DailyReportInput, unitId?: string): Promise<DailyReportResult> {
  return createDailyReport(input, unitId);
}

export async function getDailyReport(reportId: string): Promise<DailyReportResult> {
  return await safeInvoke('get_daily_report', { reportId });
}

export async function listDailyReports(startDate?: string, endDate?: string, fiscalYear?: number, month?: number): Promise<DailyReport[]> {
  return await safeInvoke('list_daily_reports', { startDate, endDate, fiscalYear, month });
}

export async function getDailyConsumption(date: string): Promise<DailyConsumptionView | null> {
  return await safeInvoke('get_daily_consumption', { date });
}

export async function calculateMealCost(items: [number, number][]): Promise<number> {
  return await safeInvoke('calculate_meal_cost', { items });
}

export async function calculateMealRate(
  totalCost: number,
  staff24hCount: number,
  staff8hCount: number,
  reservationCount: number,
  missionCount: number,
  guestCount: number,
): Promise<number> {
  return await safeInvoke('calculate_meal_rate', {
    totalCost,
    staff24hCount,
    staff8hCount,
    reservationCount,
    missionCount,
    guestCount,
  });
}

export async function recordConsumption(consumption: MealConsumptionInput): Promise<void> {
  return await safeInvoke('record_consumption', { consumption });
}
