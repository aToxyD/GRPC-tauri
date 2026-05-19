/**
 * اختبارات مكوّن AppAlert
 *
 * تغطي:
 * - عرض المحتوى وهيكل ARIA
 * - جميع متغيرات الـ intent وقيم aria-live
 * - خاصية العنوان (title)
 * - ظهور/غياب زر الإغلاق
 */
import { describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import AppAlert from '../../../lib/components/ui/AppAlert.svelte';

describe('AppAlert', () => {
  // ─── الهيكل الأساسي ───────────────────────────────────────

  it('يُعرض بـ role="alert"', () => {
    const { getByRole } = render(AppAlert, { props: { intent: 'info' } });
    expect(getByRole('alert')).toBeTruthy();
  });

  it('يحتوي على aria-live="polite" لتنبيهات info', () => {
    const { getByRole } = render(AppAlert, { props: { intent: 'info' } });
    expect(getByRole('alert').getAttribute('aria-live')).toBe('polite');
  });

  it('يحتوي على aria-live="assertive" لتنبيهات danger', () => {
    const { getByRole } = render(AppAlert, { props: { intent: 'danger' } });
    expect(getByRole('alert').getAttribute('aria-live')).toBe('assertive');
  });

  it('يحتوي على aria-live="polite" لتنبيهات warning', () => {
    const { getByRole } = render(AppAlert, { props: { intent: 'warning' } });
    expect(getByRole('alert').getAttribute('aria-live')).toBe('polite');
  });

  it('يحتوي على aria-live="polite" لتنبيهات success', () => {
    const { getByRole } = render(AppAlert, { props: { intent: 'success' } });
    expect(getByRole('alert').getAttribute('aria-live')).toBe('polite');
  });

  // ─── العنوان ──────────────────────────────────────────────

  it('يعرض العنوان عند تمريره', () => {
    const { getByText } = render(AppAlert, { props: { intent: 'info', title: 'عنوان التنبيه' } });
    expect(getByText('عنوان التنبيه')).toBeTruthy();
  });

  it('لا يعرض عنصر العنوان عند غياب title', () => {
    const { queryByText } = render(AppAlert, { props: { intent: 'info' } });
    expect(queryByText('عنوان التنبيه')).toBeNull();
  });

  // ─── زر الإغلاق ───────────────────────────────────────────

  it('لا يعرض زر الإغلاق عند dismissible=false', () => {
    const { queryByLabelText } = render(AppAlert, { props: { intent: 'info', dismissible: false } });
    expect(queryByLabelText('إغلاق التنبيه')).toBeNull();
  });

  it('يعرض زر الإغلاق عند dismissible=true', () => {
    const { getByLabelText } = render(AppAlert, { props: { intent: 'info', dismissible: true } });
    expect(getByLabelText('إغلاق التنبيه')).toBeTruthy();
  });

  it('زر الإغلاق له aria-label صحيح', async () => {
    const { getByLabelText } = render(AppAlert, { props: { intent: 'danger', dismissible: true } });
    const dismissBtn = getByLabelText('إغلاق التنبيه');
    expect(dismissBtn.getAttribute('type')).toBe('button');
  });
});
