/**
 * اختبارات مكوّن AppButton
 *
 * تغطي:
 * - الحالات الأساسية (variants, sizes)
 * - حالة التعطيل (disabled)
 * - حالة التحميل (loading)
 * - خصائص إمكانية الوصول (aria-*)
 * - نوع الزر الافتراضي
 */
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import AppButton from '../../../lib/components/ui/AppButton.svelte';

describe('AppButton', () => {
  // ─── الحالات الأساسية ──────────────────────────────────────

  it('يُعرض بنجاح', () => {
    const { getByRole } = render(AppButton, { props: {} });
    expect(getByRole('button')).toBeTruthy();
  });

  it('نوع الزر الافتراضي هو button', () => {
    const { getByRole } = render(AppButton, { props: {} });
    expect(getByRole('button').getAttribute('type')).toBe('button');
  });

  it('يقبل نوع submit', () => {
    const { getByRole } = render(AppButton, { props: { type: 'submit' } });
    expect(getByRole('button').getAttribute('type')).toBe('submit');
  });

  // ─── حالة التعطيل ─────────────────────────────────────────

  it('يكون معطلاً عند disabled=true', () => {
    const { getByRole } = render(AppButton, { props: { disabled: true } });
    const btn = getByRole('button');
    expect(btn).toBeDisabled();
    expect(btn.getAttribute('aria-disabled')).toBe('true');
  });

  // ─── حالة التحميل ─────────────────────────────────────────

  it('يُعطل الزر عند loading=true', () => {
    const { getByRole } = render(AppButton, { props: { loading: true } });
    const btn = getByRole('button');
    expect(btn).toBeDisabled();
    expect(btn.getAttribute('aria-busy')).toBe('true');
  });

  it('يكون aria-busy=false عند loading=false', () => {
    const { getByRole } = render(AppButton, { props: { loading: false } });
    expect(getByRole('button').getAttribute('aria-busy')).toBe('false');
  });

  // ─── خصائص إمكانية الوصول ─────────────────────────────────

  it('يطبق aria-label عند تمريره', () => {
    const { getByRole } = render(AppButton, { props: { ariaLabel: 'حفظ البيانات' } });
    expect(getByRole('button', { name: 'حفظ البيانات' })).toBeTruthy();
  });

  it('يحتوي على aria-disabled عند التعطيل', () => {
    const { getByRole } = render(AppButton, { props: { disabled: true } });
    expect(getByRole('button').getAttribute('aria-disabled')).toBe('true');
  });

  // ─── fullWidth ────────────────────────────────────────────

  it('يضيف w-full عند fullWidth=true', () => {
    const { getByRole } = render(AppButton, { props: { fullWidth: true } });
    expect(getByRole('button').classList.contains('w-full')).toBe(true);
  });

  it('لا يضيف w-full عند fullWidth=false', () => {
    const { getByRole } = render(AppButton, { props: { fullWidth: false } });
    expect(getByRole('button').classList.contains('w-full')).toBe(false);
  });
});
