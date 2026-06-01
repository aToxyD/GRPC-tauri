import { safeInvoke } from '../tauri';
import type {
  SupplierOrder, SupplierOrderItem, CreateOrderRequest, UpdateOrderRequest,
} from '../types';

export async function createSupplierOrder(request: CreateOrderRequest): Promise<{ orderId: string; totalAmount: number }> {
  const [orderId, totalAmount] = await safeInvoke<[string, number]>('create_supplier_order', { request });
  return { orderId, totalAmount };
}

export async function confirmOrder(orderId: string): Promise<void> {
  return await safeInvoke('confirm_order', { orderId });
}

export async function updateSupplierOrder(request: UpdateOrderRequest): Promise<number> {
  return await safeInvoke('update_supplier_order', { request });
}

export async function deleteSupplierOrder(orderId: string): Promise<void> {
  return await safeInvoke('delete_supplier_order', { orderId });
}

export async function getSupplierOrder(orderId: string): Promise<SupplierOrder | null> {
  return await safeInvoke('get_supplier_order', { orderId });
}

export async function getSupplierOrderItems(orderId: string): Promise<SupplierOrderItem[]> {
  return await safeInvoke('get_supplier_order_items', { orderId });
}

export async function listSupplierOrders(): Promise<SupplierOrder[]> {
  return await safeInvoke('list_supplier_orders');
}

export async function createOrder(request: CreateOrderRequest): Promise<{ orderId: string; totalAmount: number }> {
  const [orderId, totalAmount] = await safeInvoke<[string, number]>('create_order', { request });
  return { orderId, totalAmount };
}
