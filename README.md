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
*   **المفاتيح الإلزامية:** يجب ضبط `GRPC_APP_KEY` في بيئة الإنتاج كـ Identity صالح (يبدأ بـ `AGE-SECRET-KEY-1...`). يفشل التطبيق فوراً عند التشغيل إذا كان المفتاح غير صالح.
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

## 📚 روابط هامة
*   [التوثيق التقني (TECHNICAL_DOCUMENTATION.md)](docs/technical/TECHNICAL_DOCUMENTATION.md)
*   [سجل القرارات المعمارية (ADRs)](docs/architecture/README.md)
*   [دليل العمليات (Operator Docs)](docs/README.md)
*   [دليل المزامنة (Runbook)](docs/sync-runbook.md)

---
*هذا النظام أداة هندسية مصممة للصيانة والاستقرار، وليس للاستخدام في السحابة العامة أو التوسع اللانهائي.*
