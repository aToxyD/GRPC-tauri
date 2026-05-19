# نظام المكوّنات في منصة GRPC-Tauri
# توثيق معماري شامل

## الرؤية العامة

نظام المكوّنات هو طبقة واجهة المستخدم القياسية في المنصة. يهدف إلى:
- **إلغاء التكرار**: قواعد Tailwind موحدة في مكوّن واحد
- **توحيد السلوك**: تفاعل متسق عبر كل الصفحات
- **إمكانية الوصول**: ARIA وتنقل لوحة المفاتيح مدمجان
- **الحتمية**: رندر وتصميم متوقع دائماً
- **الحوكمة**: لا صفحة تُعيد اختراع مكوّن موجود

---

## الهيكل المعماري

```
src/lib/components/ui/
├── index.ts              ← نقطة تصدير مركزية واحدة
│
├── AppButton.svelte      ← زر الإجراء
├── AppCard.svelte        ← حاوية المحتوى
├── AppBadge.svelte       ← شارة الحالة
├── AppAlert.svelte       ← تنبيه موحد
│
├── AppFormField.svelte   ← حاوية حقل النموذج
├── AppInput.svelte       ← حقل إدخال النص
├── AppTextarea.svelte    ← حقل نص متعدد الأسطر
├── AppSelect.svelte      ← قائمة الاختيار
│
├── AppDialog.svelte      ← نافذة الحوار
├── AppTable.svelte       ← جدول البيانات
├── AppLoadingState.svelte ← حالة التحميل
├── AppEmptyState.svelte  ← حالة الفراغ
│
├── AppPageHeader.svelte  ← رأس الصفحة
└── AppSection.svelte     ← قسم المحتوى
```

---

## آلية الاستيراد القياسية

```svelte
<script lang="ts">
  // استيراد من الباريل المركزي دائماً
  import { AppButton, AppCard, AppAlert } from '../lib/components/ui';
</script>
```

**ممنوع:**
```svelte
<!-- خطأ: استيراد مباشر يكسر الحوكمة -->
import AppButton from '../lib/components/ui/AppButton.svelte';
```

---

## فلسفة المكوّنات (Component Philosophy)

### 1. المكوّن الرقيق (Thin Component)
- لا منطق أعمال داخل المكوّنات
- لا استدعاءات Tauri مباشرة
- لا حالة مشتركة مخفية
- المكوّن يُصيّر فقط ما يُعطى له

### 2. الواجهة الحتمية (Deterministic API)
كل خاصية لها قيمة افتراضية واضحة:
```svelte
export let variant: 'primary' | 'secondary' | 'danger' | 'ghost' = 'primary';
export let size: 'sm' | 'md' | 'lg' = 'md';
export let disabled = false;
export let loading = false;
```

### 3. تدفق الأحداث الصريح (Explicit Event Flow)
المكوّنات لا تتحكم في منطق الأحداث:
```svelte
<!-- المكوّن يُمرر الحدث فقط -->
<button on:click on:keydown>...</button>

<!-- الصفحة تتحكم في المنطق -->
<AppButton on:click={handleSave}>حفظ</AppButton>
```

---

## واجهات برمجة المكوّنات (Component APIs)

### AppButton
```svelte
<AppButton
  variant="primary|secondary|danger|ghost"   <!-- النمط -->
  size="sm|md|lg"                             <!-- الحجم -->
  type="button|submit|reset"                  <!-- نوع HTML -->
  disabled={false}                            <!-- تعطيل -->
  loading={false}                             <!-- تحميل -->
  fullWidth={false}                           <!-- عرض كامل -->
  ariaLabel="..."                             <!-- تسمية إمكانية الوصول -->
  on:click
  on:keydown
/>
```

### AppCard
```svelte
<AppCard
  elevated={false}           <!-- ظل مرتفع -->
  padding="sm|md|lg|none"   <!-- الحشو -->
  noBorder={false}          <!-- بدون حد -->
/>
```

### AppAlert
```svelte
<AppAlert
  intent="success|warning|danger|info"  <!-- النوع -->
  title="..."                           <!-- عنوان اختياري -->
  dismissible={false}                   <!-- قابل للإغلاق -->
  on:dismiss
/>
```

### AppBadge
```svelte
<AppBadge
  intent="success|warning|danger|info|neutral"  <!-- النوع -->
  size="sm|md"                                   <!-- الحجم -->
/>
```

### AppInput / AppTextarea / AppSelect
```svelte
<AppInput
  id="field-id"          <!-- مطلوب للربط بـ label -->
  label="..."            <!-- تسمية الحقل -->
  value=""               <!-- القيمة -->
  placeholder="..."
  required={false}
  disabled={false}
  error="..."            <!-- رسالة الخطأ -->
  helperText="..."       <!-- نص مساعد -->
  on:input
  on:change
  on:blur
/>
```

### AppDialog
```svelte
<AppDialog
  open={false}           <!-- حالة الظهور -->
  title="..."            <!-- مطلوب -->
  description="..."      <!-- اختياري -->
  size="sm|md|lg"
  destructive={false}    <!-- نمط الإجراءات المدمرة -->
  on:close
>
  <!-- محتوى الجسم -->
  <svelte:fragment slot="actions">
    <!-- أزرار الإجراءات -->
  </svelte:fragment>
</AppDialog>
```

### AppTable
```svelte
<AppTable
  loading={false}
  empty={false}
  emptyMessage="لا توجد بيانات"
  caption="وصف الجدول"
>
  <svelte:fragment slot="head">
    <th class="table-header">العمود</th>
  </svelte:fragment>

  {#each rows as row}
    <tr class="hover:bg-gray-50 dark:hover:bg-gray-700/50">
      <td class="table-cell">{row.value}</td>
    </tr>
  {/each}
</AppTable>
```

### AppPageHeader
```svelte
<AppPageHeader title="عنوان الصفحة" subtitle="وصف اختياري">
  <svelte:fragment slot="actions">
    <AppButton>إضافة</AppButton>
  </svelte:fragment>
</AppPageHeader>
```

### AppSection
```svelte
<AppSection title="عنوان القسم" description="وصف اختياري">
  <svelte:fragment slot="actions">
    <AppButton size="sm" variant="secondary">تصفية</AppButton>
  </svelte:fragment>
  <!-- المحتوى -->
</AppSection>
```

---

## تكامل نظام الثيم (Theme Integration)

### القاعدة: استخدم `dark:` variants من Tailwind فقط
```svelte
<!-- صحيح -->
<div class="bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100">

<!-- ممنوع: ألوان مشفرة مباشرة -->
<div style="background: #1f2937">

<!-- ممنوع: قيم تعسفية -->
<div class="bg-[#1a1a2e]">
```

### رموز CSS الدلالية المتاحة
```css
var(--bg-primary)      /* خلفية الصفحة الرئيسية */
var(--bg-secondary)    /* خلفية ثانوية */
var(--surface-primary) /* خلفية السطح */
var(--text-primary)    /* نص رئيسي */
var(--text-secondary)  /* نص ثانوي */
var(--border-primary)  /* حد رئيسي */
var(--color-danger)    /* لون الخطر */
var(--color-success)   /* لون النجاح */
```

---

## إمكانية الوصول (Accessibility Rules)

### قواعد إلزامية لكل مكوّن جديد

| القاعدة | التطبيق |
|---|---|
| `role` صريح | `role="alert"`, `role="dialog"` لما يلزم |
| `aria-label` | على كل زر بدون نص مرئي |
| `aria-live` | على التنبيهات الحية |
| `aria-busy` | على عناصر التحميل |
| `aria-invalid` | على حقول الإدخال الخاطئة |
| `aria-describedby` | لربط الأخطاء بحقولها |
| `aria-required` | على الحقول الإلزامية |
| `focus-visible` | على كل عنصر تفاعلي |
| HTML الدلالي | `<button>`, `<form>`, `<label>` الصحيح |

### مثال على زر مُحكم
```svelte
<button
  type="button"
  disabled={disabled || loading}
  aria-disabled={disabled || loading}
  aria-busy={loading}
  aria-label={ariaLabel}
  class="... focus-visible:ring-2 focus-visible:ring-blue-500"
>
```

---

## الأنماط المحظورة (Forbidden Patterns)

### 1. إعادة اختراع المكوّنات في الصفحات
```svelte
<!-- ممنوع في الصفحات -->
<button class="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 ...">
  حفظ
</button>

<!-- صحيح -->
<AppButton variant="primary">حفظ</AppButton>
```

### 2. تكرار أنماط Tailwind
```svelte
<!-- ممنوع: نفس النمط في 10 صفحات -->
<div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-red-700 dark:text-red-300">

<!-- صحيح -->
<AppAlert intent="danger">...</AppAlert>
```

### 3. ألوان مباشرة
```svelte
<!-- ممنوع -->
<div class="bg-[#1e40af] text-[#ffffff]">

<!-- صحيح -->
<div class="bg-civil-blue text-white">
```

### 4. منطق الثيم في الصفحات
```svelte
<!-- ممنوع -->
{#if $theme === 'dark'}
  <div class="bg-gray-800">
{:else}
  <div class="bg-white">
{/if}

<!-- صحيح: استخدم dark: variants مباشرة -->
<div class="bg-white dark:bg-gray-800">
```

### 5. استدعاء invoke مباشرة (محظور معمارياً)
```svelte
<!-- محظور بـ check_arch.ts -->
await invoke('some_command');

<!-- صحيح -->
import { someCommand } from '../lib/tauri';
await someCommand();
```

### 6. أنواع any في TypeScript (محظور معمارياً)
```typescript
// محظور بـ check_arch.ts
export let data: any;

// صحيح
export let data: Record<string, string>;
```

---

## إستراتيجية التوسع (Extension Strategy)

### إضافة مكوّن جديد
1. أنشئ ملف `AppXxx.svelte` في `src/lib/components/ui/`
2. أضفه إلى `index.ts`
3. اتبع API الموحدة (`variant`, `size`, `disabled`, `loading`)
4. أضف `aria-*` المناسبة
5. أضف اختباراً في `src/tests/unit/components/`
6. وثّق في هذا الملف

### قواعد التسمية
- الاسم: `App` + PascalCase (مثل `AppButton`, `AppDataTable`)
- لا تسميات قصيرة أو مبهمة (`Btn`, `Tbl`)
- لا تسميات تتضمن اسم الصفحة (`LoginButton`, `BackupCard`)

---

## إستراتيجية الصيانة (Maintenance Strategy)

1. **تغيير نمط موحد**: عدّله في المكوّن مرة واحدة → ينتشر في كل الصفحات
2. **إضافة variant جديد**: أضفه في `variantMap` فقط
3. **تغيير حجم**: عدّل `sizeMap` فقط
4. **إصلاح accessibility**: عدّل المكوّن مرة واحدة

---

## التحقق والاختبار

```bash
# التحقق من الأنواع والصياغة
bun run check

# تشغيل الاختبارات
bun x vitest run

# فحص الحدود المعمارية
bun run check:arch
```

---

## قواعد الحوكمة المُلزِمة

> هذه القواعد مُطبَّقة بواسطة `scripts/check_arch.ts` و مراجعة الكود.

1. ✅ كل صفحة جديدة **يجب** أن تستخدم مكوّنات `App*`
2. ✅ لا تكرار لأنماط Tailwind الموجودة في المكوّنات
3. ✅ كل مكوّن جديد يملك اختباراً واحداً على الأقل
4. ✅ لا `any` في TypeScript
5. ✅ لا `invoke` مباشر في الصفحات أو المكوّنات
6. ✅ لا ألوان مشفرة خارج `app.css`
7. ✅ كل العمليات غير المتزامنة واستدعاءات الـ IPC يجب قياسها وتتبعها بواسطة نظام الـ Telemetry (`src/lib/telemetry.ts`)
8. ✅ كل التنبيهات والإشعارات للمستخدم يجب أن تمر عبر قناة الإشعارات المركزية الموحدة (`src/lib/notifications.ts`)

---

*تم إنشاء هذا التوثيق بالعربية وفق قواعد حوكمة التوثيق في منصة GRPC-Tauri.*
