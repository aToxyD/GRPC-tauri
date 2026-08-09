# GRPC-tauri (Gestion des Restaurants de la Protection Civile)

**GRPC** هو نظام مخصص لإدارة الاستهلاك، تتبع المخزون، وإصدار التقارير لمطاعم الحماية المدنية. تم بناؤه كتطبيق سطح مكتب (Desktop App) لضمان العمل المستقل والأمان العالي في البيئات غير المتصلة بالإنترنت.

---

## 🛠️ حزمة التقنيات (Tech Stack)
*   **الواجهة الأمامية (Frontend):** Svelte 5, TypeScript, TailwindCSS.
*   **الواجهة الخلفية (Backend):** Rust (Tauri v2).
*   **قاعدة البيانات:** SQLite (WAL mode).
*   **الأمان:** age encryption (x25519), Argon2 (passwords), HMAC-SHA256 (integrity).

---

## 🚀 التشغيل السريع (Quick Start)

### المتطلبات
1.  [Rust](https://www.rust-lang.org/)
2.  [Node.js](https://nodejs.org/) & [Bun](https://bun.sh/)
3.  C++ Build Tools (Windows)

### البدء
```bash
# تثبيت التبعات
bun install

# تشغيل وضع التطوير
bun run tauri dev

# بناء النسخة النهائية
bun run tauri build
```
*ستجد الملفات المجمعة في المسار: `src-tauri/target/release/bundle/`*

---

## 🧪 الاختبارات وضمان الجودة (Testing & QA)
النظام محمي بسلسلة من الاختبارات والفحوصات المعمارية:

```bash
# 1. فحص توافق أنواع Svelte (Type Checking)
bun run check

# 2. فحص القواعد الهندسية والمعمارية الصارمة (Architecture Integrity Check)
bun run check:arch

# 3. اختبارات Rust (وحدات داخل src/ + تكامل في src-tauri/tests/)
cd src-tauri && cargo test

# اختياري قبل الدمج: تنسيق، Clippy، ثم الاختبارات
cd src-tauri && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```

---

## 🔐 النموذج الأمني (Security Model)

النظام مصمم للعمل في بيئة **معدومة الثقة (Zero-Trust)** تجاه البيانات الخارجية:

*   **التشفير:** يستخدم النظام معيار `age` (تحديداً `x25519`) حصراً. تم إزالة كافة الأساليب القديمة (legacy scrypt).
*   **المفاتيح الإلزامية:** يجب ضبط `GRPC_APP_KEY` (Identity صالح لـ `age` يبدأ بـ `AGE-SECRET-KEY-1...`) و `GRPC_PACKAGE_SIGNING_KEY` (قيمة Base64 تمثل 32 بايت تماماً) في بيئة الإنتاج (`GRPC_ENV=production`). يفشل التطبيق فوراً عند التشغيل إذا كان أي مفتاح مفقوداً أو غير صالح. لا توجد مفاتيح افتراضية في حِزم الإصدار.
*   **التحقق من النزاهة:** يتم التحقق من HMAC إجبارياً أثناء استيراد أي حزمة.
*   **التفويض:** جميع الأوامر تمر عبر دالة `authorize_command` المركزية.

### ما الذي يحميه النظام:
*   حزم البيانات المتلاعب بها (Tampered sync packages).
*   إعادة تطبيق الحزم القديمة (Replay attacks).
*   الأوامر غير المصرح بها (Unauthorized commands).
*   مسارات الملفات المشبوهة (Path traversal).

### ما الذي **لا** يحميه النظام:
*   اختراق نظام التشغيل (OS Compromise).
*   الوصول المادي للجهاز أثناء التشغيل (Physical operator abuse).
*   استخراج البيانات من الذاكرة الحية (Memory extraction).
*   مدير النظام ذو النوايا الخبيثة (Malicious admin).

---

## ⚙️ القيود التشغيلية (Operational Limits)

*   **الهدف:** نشر مكتبي محلي (Offline Desktop).
*   **الحجم المتوقع:** ولاية واحدة + حوالي 30 وحدة تابعة.
*   **نموذج التشغيل:** مشغل نشط واحد في وقت واحد.
*   **قاعدة البيانات:** الحجم الموصى به < 500MB.
*   **حزمة المزامنة:** الحد الأقصى 512MB.
*   **سجل التدقيق:** يتم تحديد التصدير بـ 50,000 سطر كحد أقصى.

---

## 🔍 التحقق من سلامة وجودة الوثائق (Documentation Integrity Verification)

يلتزم مشروع **GRPC-Tauri** بمعايير صارمة للغاية فيما يتعلق بصحة وسلامة الوثائق التقنية واستدامتها:

1. **فلسفة التوثيق التقني (Documentation Philosophy):**
   تُعامل وثائق المشروع كجزء لا يتجزأ من الكود البرمجي (Documentation-as-Code). لا تُعتبر أي ميزة برمجيّة مكتملة ومستعدة للإنتاج ما لم يصاحبها توثيق رسمي دقيق يعكس الواقع الهندسي الفعلي للمشروع.

2. **سياسة الادعاءات المستندة لأدلة مادية (Evidence-Based Claims Policy):**
   يُحظر تماماً إطلاق أي وعود أمنية أو تشغيلية فضفاضة. يجب إقران كل ادعاء تقني (مثل الذرية، أو الحتمية، أو التكرارية الآمنة) بدليل مادي من الكود المصدري الفعلي والاختبارات الآلية المرافقة له، وذلك وفق مصفوفة التحقق الرسمية [CLAIM_VERIFICATION_MATRIX.md](docs/technical/CLAIM_VERIFICATION_MATRIX.md).

3. **مرجعيات الحوكمة المعمارية (Architectural Governance References):**
   تخضع البنية البرمجية للتطبيق لرقابة صارمة بواسطة ميثاق الثوابت المعمارية [ARCHITECTURAL_INVARIANTS.md](docs/architecture/ARCHITECTURAL_INVARIANTS.md) وأداة التحقق التلقائي (`check_arch.ts`) لمنع أي انجراف أو استثناءات هيكلية غير معتمدة.

4. **سياسة الكشف الصريح عن القيود (Documentation Limitations Policy):**
   نلتزم بالأمانة والوضوح الهندسي المطلق. يتم الكشف الفوري والمباشر عن كافة القيود التشغيلية وحدود الأمان والاعتمادات الأساسية للنظام ضمن مستند [OPERATIONAL_LIMITATIONS.md](docs/technical/OPERATIONAL_LIMITATIONS.md) لضمان عدم وجود أي افتراضات مخفية أو توقعات خاطئة.

---

## 📚 روابط هامة
*   [ميثاق الحوكمة المعمارية (ARCHITECTURAL_INVARIANTS.md)](docs/architecture/ARCHITECTURAL_INVARIANTS.md)
*   [نموذج التهديدات الأمنية (THREAT_MODEL.md)](docs/security/THREAT_MODEL.md)
*   [مصفوفة التحقق من الادعاءات (CLAIM_VERIFICATION_MATRIX.md)](docs/technical/CLAIM_VERIFICATION_MATRIX.md)
*   [الحدود والموانع التشغيلية (OPERATIONAL_LIMITATIONS.md)](docs/technical/OPERATIONAL_LIMITATIONS.md)
*   [دليل حوكمة وإدارة الوثائق التقنية (DOCUMENTATION_GOVERNANCE.md)](docs/technical/DOCUMENTATION_GOVERNANCE.md)
*   [دليل حوكمة التطوير والدمج المستمر (CI_CD_GOVERNANCE.md)](docs/technical/CI_CD_GOVERNANCE.md)
*   [دليل معالجة فشل خطوط الأنابيب (PIPELINE_FAILURE_GUIDE.md)](docs/technical/PIPELINE_FAILURE_GUIDE.md)
*   [دليل حوكمة وإدارة التبعات البرمجية (DEPENDENCY_GOVERNANCE.md)](docs/technical/DEPENDENCY_GOVERNANCE.md)
*   [دليل حوكمة مستودع GitHub (GITHUB_GOVERNANCE.md)](docs/technical/GITHUB_GOVERNANCE.md)
*   [قاموس المصطلحات التقنية (GLOSSARY.md)](docs/technical/GLOSSARY.md)
*   [التوثيق التقني (TECHNICAL_DOCUMENTATION.md)](docs/technical/TECHNICAL_DOCUMENTATION.md)
*   [سجل القرارات المعمارية (ADRs)](docs/architecture/README.md)
*   [دليل العمليات (Operator Docs)](docs/README.md)
*   [دليل المزامنة (Runbook)](docs/sync-runbook.md)

---
*هذا النظام أداة هندسية مصممة للصيانة والاستقرار، وليس للاستخدام في السحابة العامة أو التوسع اللانهائي.*

