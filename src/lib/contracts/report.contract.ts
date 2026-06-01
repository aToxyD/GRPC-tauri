import { safeInvoke } from '../tauri';
import type {
  MonthlySummary, DailyReport, StockMovement, XlsxExportResult,
} from '../types';

export type ReportType = 'daily' | 'monthly' | 'stock';

export interface WilayaReportList {
  type: 'Daily' | 'Monthly' | 'Stock';
  data: DailyReport[] | MonthlySummary[] | StockMovement[];
}

export async function getMonthlySummary(year: number, month?: number): Promise<MonthlySummary> {
  return await safeInvoke('get_monthly_summary', { year, month });
}

export async function listFiscalYears(): Promise<number[]> {
  return await safeInvoke('list_fiscal_years');
}

export async function listWilayaReports(
  unitId: string | null,
  reportType: ReportType,
  year?: number,
  month?: number,
): Promise<WilayaReportList> {
  return await safeInvoke('list_wilaya_reports', { unitId, reportType, year, month });
}

export async function generateReports(type: string, month: number, year: number): Promise<void> {
  return await safeInvoke('generate_reports', { type, month, year });
}

export async function getReportData<T = unknown>(type: string, month: number, year: number): Promise<T> {
  return await safeInvoke('get_report_data', { type, month, year });
}

export async function exportDailyReportExcel(reportId: string, filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_daily_report_excel', { reportId, filePath });
}

export async function exportMonthlySummaryExcel(year: number, month: number, filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_monthly_summary_excel', { year, month, filePath });
}

export async function exportAllUnitsMonthlyStatusExcel(
  year: number,
  month: number,
  filePath: string,
): Promise<{ success: boolean; count: number; filePath: string }> {
  return await safeInvoke('export_all_units_monthly_status_excel', { year, month, filePath });
}
