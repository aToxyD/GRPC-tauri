# وثيقة حوكمة واستقرار الواجهة الأمامية

تحدد هذه الوثيقة قواعد الحوكمة الرسمية والقيود المعمارية المفروضة على طبقة الواجهة الأمامية (Frontend) في نظام **GRPC-Tauri**. يجب على جميع المطورين الالتزام الصارم بهذه السياسات للحفاظ على استقرار النظام وأمانه وقابليته للتنبؤ.

---

## 1. النموذج المعماري: واجهة أمامية خفيفة مع سيادة للمحرك الخلفي (Thin Frontend & Backend-Authoritative)

يعمل تطبيق GRPC-Tauri وفق نموذج صارم يعتمد على **واجهة أمامية خفيفة + سيادة كاملة للمحرك الخلفي**:

*   **منع منطق الأعمال في الواجهة الأمامية:** تقتصر واجهة Svelte الأمامية تماماً على كونها **طبقة عرض وتنسيق خفيفة**. يمنع منعاً باتاً كتابة منطق أعمال (Business Logic), أو حسابات مالية, أو معالجة نقل الحالات المستمرة بداخلها.
*   **خلفية Rust هي المصدر الوحيد للحقيقة:** يجب أن تُنفذ جميع العمليات الحسابية, وقواعد التحقق, وتغييرات الحالة, وفحوصات الصلاحيات والأمان داخل طبقة الـ Rust الخلفية.
*   **سلوك الإغلاق الآمن عند الفشل (Fail-Closed):** يجب أن تنغلق الواجهة الأمامية بأمان عند حدوث أي خطأ, مع تقديم رسائل واضحة ومترجمة للمستخدم دون تسريب أي تفاصيل تقنية خام للنظام.

### الأنماط البرمجية المرفوضة (ممنوعة تماماً)
*   يُمنع إدخال مكتبات إدارة الحالة المعقدة مثل **Redux** أو **Zustand** أو **MobX** أو **XState**. يجب الاكتفاء بمخازن Svelte القياسية (Svelte stores) داخل مجلد `src/lib/` (مثل `session.ts` و `theme.ts`) للحالات المرتبطة بـ UI فقط.
*   يُمنع بناء خدمات واجهة أمامية معقدة, أو نماذج نطاق (Domain Models), أو مستودعات (Repositories), أو طبقات تجريد ثقيلة في الواجهة الأمامية.

---

## 2. مركزية قنوات الاتصال البيني والاستثناءات المسموحة (Tauri IPC Boundary & Exemptions)

لمنع حدوث أخطاء استدعاء غير معالجة ولحصر نقاط الاتصال بالخلفية, يتم توحيد ومركزية قنوات الـ IPC.

### القوانين الصارمة للـ IPC:
1.  **القاعدة 27 (Rule 27):** يُمنع منعاً باتاً استدعاء `invoke(...)` التابع لـ Tauri مباشرة داخل صفحات ومكونات Svelte.
2.  **القاعدة 27ب (Rule 27b):** يُمنع منعاً باتاً الاستيراد المباشر لـ `invoke` من `@tauri-apps/api/core` أو `@tauri-apps/api` داخل صفحات ومكونات Svelte.
3.  يجب أن تمر جميع طلبات IPC للأعمال عبر التغليفات المحددة داخل الملف المشترك [src/lib/tauri.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/tauri.ts) والتي تستخدم دالة الحماية `safeInvoke`.

### الاستثناءات المسموح بها لمكتبات المنصة (Tauri Platform API Exemptions):
يُسمح حصراً بالاستيراد المباشر والاستخدام لواجهات Tauri البرمجية الخاصة بالتحكم بالنافذة والواجهة التفاعلية لنظام التشغيل والتي لا تشكل التفافاً على منطق الأعمال, وهي:
*   **مكتبة الحوارات وملفات النظام (`@tauri-apps/plugin-dialog`):** لاستدعاء `open` و `save` لفتح واجهات اختيار وحفظ الملفات (مثل استيراد حزم المزامنة أو تصدير التقارير). يجب تمرير مسار الملف المرتجع فوراً للخلفية لمعالجته, ويُمنع تماماً قراءة أو كتابة الملف من الواجهة الأمامية.
*   **التحكم بحجم وشكل النافذة (`@tauri-apps/api/window`):** لاستدعاء `getCurrentWindow()` لضبط حجم النافذة وموضعها عند الانتقال بين الصفحات (مثل تكبير النافذة بعد تسجيل الدخول أو إغلاقها).
*   **وحدات القياس والأبعاد (`@tauri-apps/api/dpi`):** لاستخدام كلاسات الأبعاد مثل `LogicalSize` أو `PhysicalSize`.
*   **مسارات النظام البرمجية (`@tauri-apps/api/path`):** لحساب مسارات الحفظ الافتراضية.

---

## 3. حماية النقرات المتعددة وأمان العمليات غير المتزامنة

لمنع حدوث سباق بيانات (Race Conditions), أو أخطاء قفل قاعدة البيانات, أو تكرار الإرسال المزدوج (Double-Submit), يستخدم النظام نظام واقي العمليات الموحد (Operation Guards).

### أدوات الحماية المتاحة:
المعرفة بالملف [src/lib/operationGuard.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/operationGuard.ts):
*   `createOperationGuard()`: يديد معالجة العمليات غير المتزامنة الحساسة للنقرات, ويمنع النقرات المزدوجة أثناء المعالجة عبر مخزن `loading`.
*   `createOperation()`: يبسط استيراد وعرض البيانات من خلال التحكم التلقائي بمخازن `loading` و `error`.

### إرشادات الاستخدام:

#### أ) للإجراءات والتغييرات (مثل الإنشاء, التعديل, المزامنة, والاسترجاع)
قم بتغليف الدوال غير المتزامنة بـ `guard(async () => { ... })` وربط حالة الأزرار بـ `$opLoading`:

```svelte
<script lang="ts">
  import { createOperationGuard } from '../lib/operationGuard';
  const { loading: opLoading, guard } = createOperationGuard();

  async function handleAction() {
    await guard(async () => {
      await executeCommand();
    });
  }
</script>

<AppButton
  loading={$opLoading}
  disabled={$opLoading}
  on:click={handleAction}
>
  تنفيذ الإجراء
</AppButton>
```

#### ب) لتحميل الصفحات والاستعلامات (جلب البيانات وعرض القوائم)
تبسيط المعالجة عبر `createOperation`:

```svelte
<script lang="ts">
  import { createOperation } from '../lib/operationGuard';
  const op = createOperation();
  const loading = op.loading;
  const error = op.error;

  async function loadData() {
    await op.run(async () => {
      data = await fetchData();
    });
  }
</script>
```

---

## 4. معالجة وتوحيد رسائل الأخطاء

يجب ألا تعرض الواجهة الأمامية أي استثناءات أو رسائل خطأ برمجية خام ومباشرة للمستخدم النهائي لمنع تسريب تفاصيل معمارية أو ثغرات في قاعدة البيانات.

*   **منع الأنماط البرمجية التالية لعرض الخطأ:** `String(e)` أو `e.message` أو `'' + e` أو `${e}`.
*   **النمط الصحيح:** يجب أن تمر كافة الأخطاء الملتقطة في كتل `catch` بداخل صفحات ومكونات Svelte عبر دالة المعالجة والترجمة الموحدة `formatErrorMessage` المعرفة في [src/lib/errors.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/errors.ts).

### مثال تطبيقي:
```typescript
import { formatErrorMessage } from '../lib/errors';

try {
  await createBackup();
} catch (err) {
  error = formatErrorMessage(err);
}
```

---

## 5. سلامة المؤقتات ودورة حياة المكونات (Timeout and Lifecycle Safety)

لتجنب تسريبات الذاكرة (Memory Leaks) وحدوث تحديثات غير متوقعة للحالة على مكونات تم إلغاء تركيبها (Unmounted Components), يجب إدارة المؤقتات بحذر.

*   **قاعدة المؤقتات:** يُمنع ترك أي مؤقت `setTimeout` أو `setInterval` يعمل بشكل مستقل دون تنظيفه عند تدمير المكون.
*   **النمط الصحيح:** يجب تتبع جميع معرفات المؤقتات النشطة داخل المكون وإلغاؤها بالكامل في دالة دورة الحياة `onDestroy`.

### مثال تطبيقي:
```svelte
<script lang="ts">
  import { onDestroy } from 'svelte';

  let success = '';
  let timers: number[] = [];

  function triggerSuccess(message: string) {
    success = message;
    const t = window.setTimeout(() => {
      success = '';
    }, 3000);
    timers.push(t);
  }

  onDestroy(() => {
    timers.forEach(clearTimeout);
  });
</script>
```

---

## 6. التدقيق التلقائي للالتزام المعماري ببيئة التطوير والمزامنة (CI/CD)

يتم تشغيل التدقيق المعماري التلقائي قبل عمليات الالتزام (Pre-commit) وفي بيئة المزامنة المستمرة عبر الأمر:

```bash
bun run check:arch
```

يقوم هذا السكريبت بفحص الكود البرمجي بالكامل ويمنع البناء (Build Failure) في الحالات التالية:
*   وجود أي ملف Svelte خارج مجلد المكتبة `src/lib/` أو الملفات المستثناة يستدعي `invoke()` مباشرة.
*   وجود أي ملف Svelte خارج مجلد المكتبة `src/lib/` أو الملفات المستثناة يستورد `invoke` من `@tauri-apps/api`.
*   وجود أي استعلامات SQL خارج طبقة المستودعات (Repositories) في Rust.
*   اعتماد كود طبقة النطاق (Domain Layer) على البنية التحتية (Infrastructure) في Rust.
