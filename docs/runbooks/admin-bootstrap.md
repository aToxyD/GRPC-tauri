# Runbook: تهيئة المسؤول (Admin Bootstrap)

# الهدف
توثيق السلوك الفعلي لإصدار حساب المسؤول الأول على عقدة جديدة، استناداً إلى السلوك المثبت عبر
إشهاد A2 (2026-08-09) على ثنائي Release.

> **تحديث (A2)**: لا يوجد مسؤول افتراضي مُبدوء مسبقاً على العقد النظيفة. العملية
> `create_default_admin` لم تعد موجودة في الكود؛ إصدار المسؤول يتم عبر أمر إصدار
> المفتاح الأول بعد إكمال إقلاع الهوية.

# النطاق
- إقلاع هوية WILAYA دون اتصال (شهادة موقّعة من Root).
- إصدار المفتاح الأول للمسؤول (`issue_first_admin_key`).
- تسجيل الدخول عبر Challenge–Response.
- لا يغطي تغيير كلمات المرور اللاحق (مسار إداري مستقل بعد تسجيل الدخول).

# السلوك الموثق (Bootstrap Semantics)

1. **لا مسؤول افتراضي (No Default Admin)**
   - على عقدة نظيفة في Release: لا يُنشأ حساب `admin` افتراضياً بكلمة مرور معروفة.
   - الوصول إلى إدارة النظام يبدأ فقط بعد إصدار مفتاح المسؤول الأول.

2. **إقلاع الهوية أولاً (Identity Provisioning First)**
   - `begin_wilaya_provision(requestFilePath)` — يولّد زوج مفاتيح العقدة، يحفظ السر في مخزن
     مفاتيح العقدة (`NodeKeyStore`)، ويكتب طلب الشهادة غير الموقّع (CSR) في ملف JSON محدد.
   - تنقل سلطة Root ملف الـCSR وتوقّع الشهادة (توقيع Ed25519 فوق
     `IdentityCertificate.canonical_bytes()`).
   - `finalize_wilaya_provision(certFilePath)` — يتحقق من توقيع Root ويعتمد هوية العقدة.
     التقديم المتكرر لنفس الشهادة هو no-op (Idempotent).
   - لإكمال الإقلاع في Release، يجب أن يُحل `GRPC_ROOT_PUBLIC_KEY` (متغير البيئة) أو
     المفتاح المضمّن (placeholder — راجع `trial-deployment-checklist.md`).

3. **إصدار المفتاح الأول للمسؤول (First Admin Key)**
   - `issue_first_admin_key(subjectUsername, passphrase)` — يُنشئ مفتاح `.adminkey` محمولاً
     محمياً بكلمة مرور (`age::scrypt`)، ويربط صف المستخدم (users) إضافياً بهوية العقدة.
   - **أول مرة فقط**: هذا الأمر مخصص لإصدار المفتاح الأول؛ لا يُعاد عبره توفير مسؤولين جدد.
   - `passphrase` إلزامي (لا يمكن أن يكون فارغاً).

4. **تسجيل الدخول (Login)**
   - بعد وجود هوية ADMIN نشطة (شهادة + `.adminkey`)، يُرفض تسجيل كلمة المرور (`login`)
     ويصبح **Challenge–Response إلزامياً**:
     - `begin_challenge()` → رسالة تحدي.
     - `complete_challenge({ sessionId, passphrase })` → عند النجاح تُنشأ الجلسة ويعود
       `LoginResponse.success = true`.
   - العقد التي لا تملك هوية ADMIN نشطة (وحدات بموجب حزمة `.unit` أو عقد غير مزوّدة) تستمر
     بمصادقة مستخدم التطبيق (`login` بكلمة المرور).

5. **لا إعادة تهيئة أبداً (Never Reset)**
   - لا يوجد مسار إقلاع يعيد إنشاء مسؤول أو يعيد تعيين كلمات المرور.
   - الاسترداد/التعديل يتم عبر مسار إداري مستقل بعد تسجيل الدخول.

# أداة توقيع Root (root-signer)

> **حدود المسؤولية (ADR-0004)**: `grpc-licensing` سلطة ترخيص **فقط**؛ **ليس** هو
> سلطة توقيع هوية Root ولا يوقّع شهادات Identity إطلاقاً. منظومة الثقة
> (`Root offline → WILAYA → UNIT/ADMIN`) ملك `grpc`، وتوقيع شهادات WILAYA يتم عبر
> أداة `root-signer` داخل `grpc/src-tauri` — وهي الأداة الوحيدة التي تحمل مفتاح
> Root الخاص (المقدَّم من بيئة/ملف خارجي، أبداً من داخل المستودع).

## البناء
```bash
cd grpc/src-tauri
cargo build --release --bin root-signer
# المخرَج: target/release/root-signer
```

## الاستخدام
```bash
root-signer sign --csr <csr.json> --out <signed.json> \
  --key-file <path-to-root-secret>   # أو GRPC_ROOT_PRIVATE_KEY، أو --key-hex
```
- مصدر مفتاح Root الخاص (واحد فقط): متغير البيئة `GRPC_ROOT_PRIVATE_KEY` أو
  `--key-file` أو `--key-hex` (32 بايت، hex أو Base64).
- إذا ضُبط `GRPC_ROOT_PUBLIC_KEY` في البيئة، تشترط الأداة أن يطابق المفتاح العام
  المشتق منه وإلا **ترفض التوقيع** (Fail-Closed).

## التدفق الكامل (Offline)
```
begin_wilaya_provision(requestFilePath)          → CSR (غير موقّع) في ملف JSON
root-signer sign --csr … --out <signed.json>     → شهادة WILAYA موقّعة (offline)
finalize_wilaya_provision(certFilePath)          → WILAYA_ACTIVE
issue_first_admin_key(subjectUsername, passphrase) → READY
begin_challenge / complete_challenge             → تسجيل دخول المسؤول
```

## ضوابط الأداة (Fail-Closed)
- `subject_type` يجب أن يكون WILAYA فقط — لا توقيع UNIT/ADMIN/حزم (Root يوقّع WILAYA حصراً).
- رفض أي CSR يحمل توقيعاً مسبقاً.
- إجبار `algorithm_version` على ملف الهوية Ed25519.
- رفض الشهادات غير النشطة (غير ACTIVE).
- التحقق الذاتي من التوقيع قبل كتابة الملف الموقَّع.
- مفتاح Root الخاص لا يُطبع أبداً على stdout/stderr/logs.

## ملاحظة أمنية حاسمة
اختبار نهاية-إلى-نهاية بمفتاح **RFC 8032 §7.1 TEST 1** يثبت صحة البروتوكول فقط؛
**ليس** دليلاً على الجاهزية الإنتاجية. الإنتاج يتطلب مفتاح Root خاصاً حقيقياً يطابق
مفتاحاً عاماً حقيقياً عبر `GRPC_ROOT_PUBLIC_KEY` (راجع
`security-production-keys.md`). حِزم Release من الأداة ترفض التوقيع بمفاتيح الاختبار
المعروفة (TEST 1/TEST 2).

# الضمان الأمني
> **إصدار المسؤول مرتبط بهوية العقدة المزوّدة؛ لا يوجد حساب افتراضي قابل للتخمين،
> والمصادقة بعد الإصدار عبر Challenge–Response (توقيع) وليس كلمة مرور ثابتة.**

# التحقق
1. ابدأ عقدة نظيفة وأنشئ مفتاح التطبيق (شاشة إعداد الأمان).
2. نفّذ `begin_wilaya_provision` → وقّع الـCSR عبر سلطة Root → `finalize_wilaya_provision`.
3. نفّذ `issue_first_admin_key` بمستخدم وكلمة مرور، ثم سجّل الدخول عبر
   `begin_challenge` / `complete_challenge`.
4. تأكد أن محاولة `login` بكلمة المرور مرفوضة بعد إصدار الهوية النشطة.
5. أعد تشغيل التطبيق وتأكد من استمرار إمكانية الدخول عبر Challenge–Response (لا إعادة إصدار).
