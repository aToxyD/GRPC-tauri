import { safeInvoke } from '../tauri';
import type {
  Product, CreateProductRequest, UpdateProductRequest,
  Unit, CreateUnitRequest, InventoryStock,
  StockCheckResult, ConsumptionItemInput,
  StockMovementFilters, StockMovementResponse, StockSummary, StockMovement,
  ComputeSnapshotResult, UnitInventoryView, InventoryStockPageView,
  XlsxExportResult,
} from '../types';

export async function createProduct(request: CreateProductRequest): Promise<string> {
  return await safeInvoke('create_product', { request });
}

export async function updateProduct(request: UpdateProductRequest): Promise<void> {
  return await safeInvoke('update_product', { request });
}

export async function deleteProduct(productId: string): Promise<void> {
  return await safeInvoke('delete_product', { productId });
}

export async function getProduct(productId: string): Promise<Product | null> {
  return await safeInvoke('get_product', { productId });
}

export async function listProducts(): Promise<Product[]> {
  return await safeInvoke('list_products');
}

export async function createUnit(request: CreateUnitRequest, wilayaCode: string): Promise<Unit> {
  return await safeInvoke('create_unit', { request, wilayaCode });
}

export async function getUnit(unitId: string): Promise<Unit | null> {
  return await safeInvoke('get_unit', { unitId });
}

export async function listUnits(wilayaCode: string): Promise<Unit[]> {
  return await safeInvoke('list_units', { wilayaCode });
}

export async function updateUnit(unitId: string, request: CreateUnitRequest): Promise<void> {
  return await safeInvoke('update_unit', { unitId, request });
}

export async function deleteUnit(unitId: string): Promise<void> {
  return await safeInvoke('delete_unit', { unitId });
}

export async function getStock(productId: string): Promise<InventoryStock | null> {
  return await safeInvoke('get_stock', { productId });
}

export async function getAllStocks(): Promise<InventoryStock[]> {
  return await safeInvoke('get_all_stocks');
}

export async function checkStockAvailability(items: ConsumptionItemInput[]): Promise<StockCheckResult[]> {
  return await safeInvoke('check_stock_availability', { items });
}

export async function getCurrentStock(): Promise<InventoryStock[]> {
  return await safeInvoke('get_current_stock');
}

export async function getInventoryFifoView(): Promise<InventoryStockPageView> {
  return await safeInvoke('get_inventory_fifo_view');
}

export async function computeUnitInventorySnapshot(
  unitId: string,
  year: number,
  month: number,
  forceRecompute = false,
): Promise<ComputeSnapshotResult> {
  return await safeInvoke('compute_unit_inventory_snapshot', { unitId, year, month, forceRecompute });
}

export async function getUnitInventoryView(
  unitId: string,
  year: number,
  month: number,
): Promise<UnitInventoryView | null> {
  return await safeInvoke('get_unit_inventory_view', { unitId, year, month });
}

export async function getAvailableReportMonths(unitId: string): Promise<[number, number][]> {
  return await safeInvoke('get_available_report_months', { unitId });
}

export async function exportUnitInventoryExcel(
  unitId: string,
  year: number,
  month: number,
  filePath: string,
): Promise<XlsxExportResult> {
  return await safeInvoke('export_unit_inventory_excel', { unitId, year, month, filePath });
}

export async function getStockMovements(
  filters: StockMovementFilters = {},
  page: number = 0,
  pageSize: number = 50,
): Promise<StockMovementResponse> {
  return await safeInvoke('get_stock_movements', { filters, page, pageSize });
}

export async function getStockSummary(): Promise<StockSummary[]> {
  return await safeInvoke('get_stock_summary');
}

export async function calculateProductPriceWithTva(basePrice: number, tva: number): Promise<number> {
  return await safeInvoke('calculate_product_price_with_tva', { basePrice, tva });
}

export async function exportProductsExcel(filePath: string): Promise<XlsxExportResult> {
  return await safeInvoke('export_products_excel', { filePath });
}

export async function exportStockMovementsExcel(
  productId: string | undefined,
  filePath: string,
): Promise<XlsxExportResult> {
  return await safeInvoke('export_stock_movements_excel', { productId, filePath });
}

export async function getOrders(): Promise<import('../types').SupplierOrder[]> {
  return await safeInvoke('get_orders');
}
