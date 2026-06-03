# نظام الثيم في منصة GRPC-Tauri
# توثيق معماري شامل

## نظرة عامة

نظام الثيم في منصة GRPC-Tauri مصمم وفق مبادئ:
- **المركزية الكاملة**: جميع منطق الثيم محصور في `src/lib/theme.ts`
- **الحتمية**: سلوك ثابت وقابل للتنبؤ عند كل بدء تشغيل
- **إمكانية الوصول**: دعم كامل لمعايير WCAG 2.1
- **الأداء**: لا وميض (FOUC)، لا تأخير في الرندر

---

## الهيكل المعماري

```
منصة GRPC-Tauri
│
├── index.html                  ← تهيئة مبكرة (قبل تحميل Svelte)
│
├── src/
│   ├── lib/
│   │   └── theme.ts            ← المصدر الوحيد للحقيقة (Single Source of Truth)
│   │
│   ├── components/
│   │   └── Sidebar.svelte      ← زر التبديل الوحيد
│   │
│   └── app.css                 ← رموز التصميم الدلالية + مكوّنات CSS
```

---

## الثيم الافتراضي

**الثيم الافتراضي = داكن**

المنطق عند بدء التشغيل (محدد في `index.html`):

```
1. هل يوجد تفضيل محفوظ في localStorage؟
   └── نعم: استخدمه مباشرة
   └── لا:  → الوضع الداكن (الافتراضي المطلق)
```

> **ملاحظة**: حتى لو كان تفضيل النظام `prefers-color-scheme: light`، يظل الوضع الداكن هو الافتراضي عند غياب تفضيل محفوظ.

---

## رموز التصميم الدلالية (Semantic Design Tokens)

تُعرَّف في `src/app.css` ضمن `:root` و `.dark`:

### الخلفيات
| الرمز | الثيم الفاتح | الثيم الداكن |
|---|---|---|
| `--bg-primary` | #f9fafb (gray-50) | #111827 (gray-900) |
| `--bg-secondary` | #f3f4f6 (gray-100) | #1f2937 (gray-800) |
| `--surface-primary` | #ffffff | #1f2937 (gray-800) |
| `--surface-elevated` | #ffffff | #374151 (gray-700) |

### النصوص
| الرمز | الثيم الفاتح | الثيم الداكن |
|---|---|---|
| `--text-primary` | #111827 (gray-900) | #f9fafb (gray-50) |
| `--text-secondary` | #6b7280 (gray-500) | #9ca3af (gray-400) |
| `--text-muted` | #9ca3af (gray-400) | #6b7280 (gray-500) |

### الحدود
| الرمز | الثيم الفاتح | الثيم الداكن |
|---|---|---|
| `--border-primary` | #e5e7eb (gray-200) | #374151 (gray-700) |
| `--border-secondary` | #d1d5db (gray-300) | #4b5563 (gray-600) |

### الألوان الدلالية
| الرمز | الثيم الفاتح | الثيم الداكن |
|---|---|---|
| `--color-danger` | #dc2626 (red-600) | #f87171 (red-400) |
| `--color-success` | #16a34a (green-600) | #4ade80 (green-400) |
| `--color-warning` | #ca8a04 (yellow-600) | #facc15 (yellow-400) |
| `--color-info` | #2563eb (blue-600) | #60a5fa (blue-400) |

---

## منع وميض الثيم الخاطئ (FOUC Prevention)

### المشكلة
إذا تم تطبيق الثيم بعد تحميل Svelte، يظهر وميض أبيض لحظي.

### الحل
يوجد في `index.html` script مُضمَّن يُنفَّذ **قبل** أي تحميل لـ JavaScript:

```html
<script>
  try {
    var saved = localStorage.getItem('theme');
    if (saved === 'light') {
      document.documentElement.classList.remove('dark');
    } else {
      document.documentElement.classList.add('dark');
    }
  } catch (_) {
    document.documentElement.classList.add('dark');
  }
</script>
```

**ضمانات**:
- يُنفَّذ قبل أي CSS أو JavaScript
- محصور في try/catch لتجنب الأعطال
- لا يعتمد على أي مكتبة خارجية

---

## انتقالات الثيم (Theme Transitions)

### المسموح به
- `background-color 200ms ease`
- `color 200ms ease`
- `border-color 200ms ease`

### المحظور
- انيميشن التخطيط (layout animations)
- تأخير أكثر من 300ms
- انيميشن على كل العناصر

### احترام `prefers-reduced-motion`
```css
@media (prefers-reduced-motion: reduce) {
  body, .card, .input-field, ... {
    transition: none !important;
  }
}
```

---

## إمكانية الوصول (Accessibility)

### حلقات التركيز
جميع العناصر التفاعلية تملك `focus-visible` ring واضح:
```css
:focus-visible {
  outline: 2px solid var(--color-civil-blue);
  outline-offset: 2px;
}
.dark :focus-visible {
  outline-color: #60a5fa; /* blue-400 — تباين أعلى في الوضع الداكن */
}
```

### تسميات ARIA
- أزرار التبديل: `aria-label` + `title`
- رسائل الخطأ: `role="alert"`
- الإشعارات الحية: `role="status" aria-live="polite"`
- الحقول الإلزامية: `aria-required="true"`
- الحالة المحمّلة: `aria-busy={loading}`

### تباين الألوان
الألوان الدلالية في الوضع الداكن مُختارة لضمان نسبة تباين ≥ 4.5:1 وفق WCAG AA.

---

## استمرارية الثيم (Theme Persistence)

### آلية الحفظ
```typescript
// في theme.ts — يحدث تلقائياً عند كل تغيير
localStorage.setItem('theme', value);
```

### آلية الاسترجاع
```typescript
// في initializeTheme()
const saved = localStorage.getItem('theme') as Theme | null;
const initial: Theme = saved === 'light' ? 'light' : 'dark';
```

### ضمانات
- الثيم يُستعاد **قبل** رندر أي مكوّن
- لا وميض بعد إعادة التشغيل
- المنطق حتمي ولا يعتمد على توقيت

---

## قواعد الحوكمة (Governance Rules)

### ✅ المسموح به
```svelte
<!-- استخدام Tailwind dark variants -->
<div class="bg-white dark:bg-gray-800 text-gray-900 dark:text-white">

<!-- استخدام رموز CSS المركزية -->
<div style="color: var(--text-primary)">

<!-- استخدام مكوّنات CSS المحددة -->
<button class="btn-primary">
```

### ❌ المحظور تماماً

**1. ألوان مشفرة عشوائية**
```svelte
<!-- خطأ -->
<div style="background-color: #1a1a2e">

<!-- خطأ -->
<div style="color: rgb(200, 200, 200)">
```

**2. منطق الثيم في الصفحات**
```svelte
<!-- خطأ: لا تتحقق من الثيم في الصفحات -->
{#if $theme === 'dark'}
  <div class="bg-black">
{:else}
  <div class="bg-white">
{/if}
```

**3. تلاعب مباشر بـ DOM**
```typescript
// خطأ: لا تُعدّل classList مباشرة خارج theme.ts
document.documentElement.classList.add('dark'); // محظور في الصفحات
```

**4. class: directive مع مسافات**
```svelte
<!-- خطأ: class: لا يدعم مسافات -->
<div class:bg-blue-50 dark:bg-blue-900={condition}>

<!-- صحيح: استخدم class="" مع ternary -->
<div class="{condition ? 'bg-blue-50 dark:bg-blue-900' : ''}">
```

**5. استيراد theme.ts في الصفحات للتحقق فقط**
```typescript
// خطأ: لا تستورد theme في الصفحات لمجرد إضافة class
import { theme } from '../lib/theme'; // محظور في الصفحات
```

---

## بنية theme.ts

```typescript
// src/lib/theme.ts
export type Theme = 'light' | 'dark';

// المتجر — الثيم الافتراضي: داكن
const { subscribe, set, update } = writable<Theme>('dark');

// initializeTheme() — تُستدعى مرة واحدة عند بدء التطبيق
export function initializeTheme(): void {
  const saved = localStorage.getItem('theme');
  const initial: Theme = saved === 'light' ? 'light' : 'dark';
  theme.set(initial);
}

// toggleTheme() — تُستدعى من Sidebar فقط
export function toggleTheme(): void {
  theme.toggle();
}
```

---

## قواعد الصيانة (Maintenance Rules)

1. **إضافة صفحة جديدة**: استخدم `dark:` variants مباشرة — لا تُعدّل `theme.ts`
2. **إضافة لون جديد**: أضفه في `:root` و `.dark` في `app.css` — لا تُفرد ألواناً
3. **تغيير الثيم الافتراضي**: عدّل فقط `index.html` و `theme.ts` (writable initial value + fallback)
4. **إضافة انتقال**: أضفه في `app.css` ضمن `--transition-theme` — لا تُفرد انتقالات
5. **اختبار الثيم**: عدّل `src/tests/theme.test.ts` — لا تنشئ ملفات اختبار موزعة

---

## التحقق والاختبار

```bash
# التحقق من الأنواع
bun run check

# تشغيل اختبارات الثيم
bun x vitest run src/tests/theme.test.ts

# فحص الحدود المعمارية
bun run check:arch
```

---

## الأنماط المحظورة في check_arch.ts

> هذه القواعد مُطبَّقة تلقائياً — أي انتهاك يمنع الدمج.

- لا SQL في ملفات الواجهة
- لا `invoke` مباشر في الصفحات (يجب المرور عبر `src/lib/tauri.ts`)
- لا أنواع `any` في TypeScript
- لا استيراد مستودعات مباشرة في الأوامر

---

*تم إنشاء هذا التوثيق بالعربية وفق قواعد حوكمة التوثيق في منصة GRPC-Tauri.*
