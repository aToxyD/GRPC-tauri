# RFC: Backup Command Async Exception (استثناء async في أوامر النسخ الاحتياطي)

**تاريخ:** 2026-08-09
**الحالة:** ✅ **Approved** — اعتمد هذا الـ RFC بتاريخ 2026-08-09. القرار: استثناء معماري دائم ضيّق، لا إزالة لـ`spawn_blocking`، لا تحويل backup/restore إلى synchronous، والاستثناء محصور في نقطتي الدخول `create_backup` و`restore_backup` داخل `commands/backup.rs` فقط.
**النطاق:** Frozen contract §2.6 "No async" في `docs/architecture/ARCHITECTURE_FREEZE.md` — carve-out ضيّق ودائم.
**المالك:** Architecture
**المراجع:** ADR-006 → `docs/architecture/0026-no-async-runtime.md`؛ سياسة الاستثناءات → ADR-0030 → `docs/architecture/0030-adr-exception-governance.md`؛ العملية الرسمية → `ARCHITECTURE_FREEZE.md` Section 4 (RFC-to-ADR).

> **إعلان حوكمي:** اعتماد هذا الـRFC يُفعّل الخطوة 3 من عملية `ARCHITECTURE_FREEZE.md` Section 4. المسودة الواردة في القسمين 3 و5 أدناه أصبحت **معتمدة للتنفيذ** عبر التسلسل: مراجعة RFC → إنشاء ADR-0043 → تعديل freeze §2.6 → توسيع Rule 117 → إضافة suppression metadata → تسجيل في الـregistry كـPermanent → اختبارات → full gates → مراجعة diff مستقلة → commit. لا يبدأ التنفيذ البرمجي إلا بعد اكتمال مراجعة RFC والموافقة عليه.

---

## 1. Problem Statement (بيان المشكلة)

### 1.1 الوضع الحالي للمشكلة الجوهرية

`src-tauri/src/commands/backup.rs` يحتوي **السطح الوحيد** للـasync في شجرة Rust بأكملها:

- `create_backup` — `pub async fn` عند السطر 46.
- `restore_backup` — `pub async fn` عند السطر 357.
- `tauri::async_runtime::spawn_blocking` عند السطرين 193 و456.
- `.await` عند السطرين 194 و459.

الغرض من الـasync هو إخراج عملية النسخ الاحتياطي/الاستعادة (نسخ قاعدة البيانات + التشفير — I/O حاجب) من خيط أوامر Tauri الرئيسي عبر `spawn_blocking`، تجنبًا لتجميد واجهة المستخدم.

هذا الاستخدام **يخالف** بنودًا مجمّدة حاليًا:

1. `docs/architecture/ARCHITECTURE_FREEZE.md` §2.6 السطر 144: **🔒 No async** — لا `async fn` ولا `await` ولا `tokio` ولا `futures` في أي طبقة Rust.
2. Tenet #3 (السطر 83): No async runtime — كل كود Rust متزامن.
3. `AGENTS.md` Runtime Tenet #2: `async fn`, `await`, `tokio`, `futures` ممنوعة في أي طبقة Rust.
4. `ADR-006` قرار 1–2: عدم وجود async في أي ملف مصدر Rust، وكل Tauri command handler متزامن.

**الفجوة الحوكمية الجوهرية:** المخالفة **غير مرئية** لأداة `bun run check:arch` اليوم لأن Rule 117 في `scripts/check_arch.ts` (السطر 1677) يمسح فقط `src-tauri/src/infrastructure/sqlite_runtime_review/**/*.rs` ولا يغطي `commands/`. هذا انحراف غير مُكتشف (undetected drift) بموجب `AGENTS.md` C3 — الانحراف موجود منذ الـinitial commit `a93a96b`، والـdeadlock fix في `e0f0cca` (2026-06-03).

## 2. Current Behavior (السلوك الحالي — العقد المجمّد)

- لا `async fn` / `await` / `tokio` / `futures` في أي طبقة Rust (FREEZE §2.6، AGENTS.md Tenet #2، ADR-006 قرار 1–2).
- Rule 117 يفرض ذلك حاليًا **داخل `sqlite_runtime_review/` فقط**.
- النسخ الاحتياطي/الاستعادة عمليات ملفات حاجبة (DB copy + encryption). في Tauri 2، أوامر الـsync تعمل على الـmain thread → تنفيذ متزامن لها من شأنه تجميد واجهة المستخدم أثناء النسخ/الاستعادة. هذا الأثر موثّق أصلًا في ADR-006 ضمن "Consequences-Harder".

## 3. Proposed Change (التغيير المقترح)

استثناء معماري **ضيّق ودائم** عبر عملية RFC-to-ADR:

- **الـasync مسموح فقط عند نقطتي الدخول** `create_backup` و`restore_backup` في `src-tauri/src/commands/backup.rs`، و**فقط** من أجل `tauri::async_runtime::spawn_blocking` لعمليات النسخ الاحتياطي/الاستعادة الحاجبة (blocking backup/restore I/O).
- **لا يُسمح بتسريب async** إلى Domain (`src-tauri/src/domain/ports/backup.rs` يبقى متزامنًا بالكامل)، ولا إلى Application/Services، ولا إلى بقية Commands.
- **الإنفاذ:** توسيع Rule 117 لمسح كل `src-tauri/src/**/*.rs` مع استثناء **ملف محدد** — `commands/backup.rs` (المسار الدقيق، وليس pattern عامًا لـ`commands/*`) — بالإضافة إلى تحقق الـengine من وسوم `[arch:allow-async]`.

## 4. Impact Analysis (تحليل الأثر — بنود Section 2 المتأثرة)

| البند المجمّد | الأثر |
|---|---|
| `ARCHITECTURE_FREEZE.md` §2.6 السطر 144 "🔒 No async" | يُعدَّل بإضافة carve-out (المسودة أدناه) |
| Tenet #3 (السطر 83) | يبقى دون تغيير (بيان عام) — §2.6 يحمل الاستثناء |
| `ADR-006` قرار 1–2 | يُجاوز جزئيًا بالنسبة لمعالِجَي أوامر backup الثلاثة؛ ADR-0043 يربط الـRFC |
| `ADR-0030` سياسة استحقاق 90 يومًا | يُعدَّل: `[arch:allow-async]` يُسجَّل كـ**Permanent** — الـvalidator يبقي قاعدة 90 يومًا افتراضية لكل الوسوم الأخرى |

## 5. Proposed Freeze §2.6 Amendment (مسودة تعديل التجميد)

```
- **🔒 No async** — no `async fn`, `await`, `tokio`, `futures` in any Rust layer.
  **Sanctioned exception (ADR-0043):** `tauri::async_runtime::spawn_blocking` for blocking
  backup/restore file I/O in `src-tauri/src/commands/backup.rs` ONLY — `create_backup` and
  `restore_backup` are the sole async entry points; no async beyond these two command handlers,
  and none in Domain/Application/Ports.
```

## 6. Migration Plan (خطة الهجرة)

1. إنشاء هذا الـRFC (الحالي) → موافقة.
2. إنشاء ADR-0043 (Status: Accepted، يربط الـRFC).
3. تعديل freeze §2.6 وفق المسودة أعلاه.
4. توسيع Rule 117 في `scripts/check_arch.ts` + إضافة وسوم `[arch:allow-async]` عند `backup.rs:46,:357`.
5. تسجيل صفوف في `docs/architecture/adr_exception_registry.md` معلَّمة **Permanent**.
6. تمديد الـvalidator (`scripts/governance/suppression.ts`) للاستثناء الدائم — تحقق يعتمد على وجود ADR مسجَّل/صالح، مع `0043` كأول حالة، وليس مجرد marker.
7. إضافة اختبارات حوكمة (permanent → لا فشل استحقاق؛ ordinary → 90 يومًا؛ permanent بلا registry/ADR → فشل).

## 7. Rollback Plan (خطة التراجع)

إزالة الوسوم + صفوف الـregistry → التراجع عن تعديل freeze §2.6 → التراجع عن توسيع Rule 117 → التراجع عن تعديل الـvalidator. أوامر backup تبقى وظيفية (الوسوم تعليقات فقط؛ لا تغيير في السلوك).

## 8. Backward Compatibility (التوافق الرجعي)

صفر تغيير سلوكي — الوسوم تعليقات فقط؛ مسار `spawn_blocking` دون تغيير؛ عقود الواجهة الأمامية (`create_backup`/`restore_backup` عبر `src/lib/tauri.ts` و`src/lib/contracts/backup.contract.ts`) دون تغيير.

---

## Appendix A — تصميم الاستثناء الدائم في الـvalidator

- **Registry:** صفوف `[arch:allow-async]` معلَّمة `Permanent`، ADR=0043.
- **تغيير الـvalidator** (`validateSuppressionMetadata` في `scripts/governance/suppression.ts`): يُعفى الوسم من استحقاق 90 يومًا **فقط إذا** حمل marker `Permanent: ADR-NNNN`، وذكر ADR مرجِع موجود ومسجَّل/صالح في `PERMANENT_ADRS` (تُسقى بـ`0043`)، وامتلك metadata كاملة (Reason/Date/Owner) وربط `see ADR-0043`. جميع الوسوم الأخرى تبقى على قاعدة 90 يومًا الافتراضية.
- **حارس عدم الانفصال عن سجل الـADR:** `PERMANENT_ADRS` ليست قائمة يدوية منفصلة — يتحقق الـvalidator من صحة/تسجيل الـADR المرجعي نفسه، مع `0043` كأول حالة، بدل منح أي وسم صلاحية دائمة بمجرد كتابة marker.
- **الاختبارات:**
  1. permanent exception (Date قديم) → لا فشل استحقاق.
  2. ordinary exception (Date قديم) → لا يزال يخضع لـ90 يومًا.
  3. permanent tag بلا registry/ADR صالح → فشل.

## Appendix B — مسودة توسيع Rule 117

```ts
checkRule(
  "Rule 117: Async runtime usage in Rust (must be synchronous)",
  ["src-tauri/src/**/*.rs"],
  /\basync\s+fn\b|\bawait\b|\btokio::\b|\bfutures::\b|\bAsync\b|spawn_blocking/,
  (line) => line.trim().startsWith("//"),
  "error",
  (f) => !f.includes("src-tauri/src/commands/backup.rs")
);
```

- الاستثناء **ملف محدد** (`commands/backup.rs` حصريًا)، وليس pattern عامًا لـ`commands/*`.
- تحقُّق أُجري: صفر مطابقات خارج `backup.rs` و`sqlite_runtime_review/` في الشجرة الحالية → التوسيع آمن.

## Appendix C — مرتكزات موثّقة

- `docs/architecture/0026-no-async-runtime.md` (ADR-006) — السياسة المخالفة المراد استثناؤها جزئيًا.
- `docs/architecture/0030-adr-exception-governance.md` (ADR-0030) — سياسة الاستحقاق والـregistry.
- `docs/architecture/ARCHITECTURE_FREEZE.md` §4 (RFC-to-ADR) — العملية المعتمدة لتعديل بند مجمّد.
- `docs/architecture/adr_exception_registry.md` — سجل الوسوم (سيُضاف قسم `[arch:allow-async]`).
- `scripts/check_arch.ts` — Rule 117 (السطر 1677) و`checkRule` (السطر 40).
- `scripts/governance/invariants/runtimeSafety.ts` — كتلة B3-2 لوسوم Rust (:463–492).
- `scripts/governance/suppression.ts` — `validateSuppressionMetadata` و`collectSuppressions`.
