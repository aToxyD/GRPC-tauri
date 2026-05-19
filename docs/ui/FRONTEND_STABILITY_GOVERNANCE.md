# وثيقة حوكمة واستقرار الواجهة الأمامية

تحدد هذه الوثيقة قواعد الحوكمة الرسمية والقيود المعمارية المفروضة على طبقة الواجهة الأمامية (Frontend) في نظام **GRPC-Tauri**. يجب على جميع المطورين الالتزام الصارم بهذه السياسات للحفاظ على استقرار النظام وأمانه وقابليته للتنبؤ.

---

## 1. النموذج المعماري: واجهة أمامية خفيفة مع سيادة للمحرك الخلفي (Thin Frontend & Backend-Authoritative)

يعمل تطبيق GRPC-Tauri وفق نموذج صارم يعتمد على **واجهة أمامية خفيفة + سيادة كاملة للمحرك الخلفي**:

*   **منع منطق الأعمال في الواجهة الأمامية:** تقتصر واجهة Svelte الأمامية تماماً على كونها **طبقة عرض وتنسيق خفيفة**. يمنع منعاً باتاً كتابة منطق أعمال (Business Logic)، أو حسابات مالية، أو معالجة نقل الحالات المستمرة بداخلها.
*   **خلفية Rust هي المصدر الوحيد للحقيقة:** يجب أن تُنفذ جميع العمليات الحسابية، وقواعد التحقق، وتغييرات الحالة، وفحوصات الصلاحيات والأمان داخل طبقة الـ Rust الخلفية.
*   **سلوك الإغلاق الآمن عند الفشل (Fail-Closed):** يجب أن تنغلق الواجهة الأمامية بأمان عند حدوث أي خطأ، مع تقديم رسائل واضحة ومترجمة للمستخدم دون تسريب أي تفاصيل تقنية خام للنظام.

### الأنماط البرمجية المرفوضة (ممنوعة تماماً)
*   يُمنع إدخال مكتبات إدارة الحالة المعقدة مثل **Redux** أو **Zustand** أو **MobX**. يجب الاكتفاء بمخازن Svelte القياسية (Svelte stores) داخل مجلد `src/lib/` (مثل `session.ts` و `theme.ts`) للحالات المرتبطة بـ UI فقط.
*   يُمنع بناء خدمات واجهة أمامية معقدة، أو نماذج نطاق (Domain Models)، أو مستودعات (Repositories)، أو طبقات تجريد ثقيلة في الواجهة الأمامية.

---

## 2. مركزية قنوات الاتصال البيني (Tauri IPC Boundary - Rule 27 & 27b)

لمنع حدوث أخطاء استدعاء غير معالجة ولحصر نقاط الاتصال بالخلفية، يتم توحيد ومركزية قنوات الـ IPC.

### القواعد الصارمة:
1.  **القاعدة 27 (Rule 27):** يُمنع منعاً باتاً استدعاء `invoke(...)` التابع لـ Tauri مباشرة داخل صفحات ومكونات Svelte.
2.  **القاعدة 27ب (Rule 27b):** يُمنع منعاً باتاً الاستيراد المباشر من `@tauri-apps/api` أو `@tauri-apps/api/core` داخل صفحات ومكونات Svelte.

### آلية التطبيق السليم:
يجب أن تمر جميع طلبات IPC عبر التغليفات والمخرجات المحددة داخل الملف المشترك [src/lib/tauri.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/tauri.ts):

*   **النمط الصحيح:**
    ```typescript
    import { listBackups } from '../lib/tauri';
    const backups = await listBackups();
    ```
*   **النمط الخاطئ (مخالفة معمارية):**
    ```typescript
    import { invoke } from '@tauri-apps/api/core'; // مخالفة Rule 27b
    const backups = await invoke('list_backups');  // مخالفة Rule 27
    ```

---

## 3. حماية النقرات المتعددة وأمان العمليات غير المتزامنة

لمنع حدوث سباق بيانات (Race Conditions)، أو أخطاء قفل قاعدة البيانات، أو تكرار الإرسال المزدوج (Double-Submit)، يستخدم النظام نظام واقي العمليات الموحد (Operation Guards).

### أدوات الحماية المتاحة:
المعرفة بالملف [src/lib/operationGuard.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/operationGuard.ts):
*   `createOperationGuard()`: يدير معالجة العمليات غير المتزامنة الحساسة للنقرات، ويمنع النقرات المزدوجة أثناء المعالجة عبر مخزن `loading`.
*   `createOperation()`: يبسط استيراد وعرض البيانات من خلال التحكم التلقائي بمخازن `loading` و `error`.

### إرشادات الاستخدام:

#### أ) للإجراءات والتغييرات (مثل الإنشاء، التعديل، المزامنة، والاسترجاع)
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

يجب ألا تعرض الواجهة الأمامية أي استثناءات أو رسائل خطأ برمجية خام ومباشرة للمستخدم النهائي.

*   يجب أن تمر كافة الأخطاء الملتقطة في كتل `catch` بداخل صفحات ومكونات Svelte عبر دالة المعالجة والترجمة الموحدة `formatErrorMessage` المعرفة في [src/lib/errors.ts](file:///c:/Users/atoxyd/Desktop/1Bxm/GRPC-tauri/src/lib/errors.ts).
*   تقوم الدالة بترجمة الأخطاء التشغيلية والفنية المرتجعة من قاعدة البيانات أو العمليات الخلفية لرسائل عربية مفهومة تناسب المستخدم.

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

## 5. التدقيق التلقائي للالتزام المعماري ببيئة التطوير والمزامنة (CI/CD)

يتم تشغيل التدقيق المعماري التلقائي قبل عمليات الالتزام (Pre-commit) وفي بيئة المزامنة المستمرة عبر الأمر:

```bash
bun run check:arch
```

يقوم هذا السكريبت بفحص الكود البرمجي بالكامل ويمنع البناء (Build Failure) في الحالات التالية:
*   وجود أي ملف Svelte خارج مجلد المكتبة `src/lib/` يستدعي `invoke()` مباشرة.
*   وجود أي ملف Svelte خارج مجلد المكتبة `src/lib/` يستورد من `@tauri-apps/api`.
*   وجود أي استعلامات SQL خارج طبقة المستودعات (Repositories) في Rust.
*   اعتماد كود طبقة النطاق (Domain Layer) على البنية التحتية (Infrastructure) في Rust.
