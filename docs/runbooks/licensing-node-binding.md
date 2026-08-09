# Runbook: سلوك ربط الترخيص بالعقدة (Node-A / Node-B Binding)

# الهدف
توثيق سلوك الربط بين الترخيص والعقدة كما أُثبت عملياً في إشهاد A2 (2026-08-09): لماذا
الترخيص صالح على العقدة A ومرفوض على العقدة B رغم توقيعه الصحيح ونفس المرساة.

# النطاق
- قاعدة الربط (Subject Binding).
- سلوك العقدة المرخّصة (Node A) مقابل العقدة غير المرخّصة (Node B).
- الأثر التشغيلي لإعادة تزويد الهوية (تغيير مفتاح العقدة).

# مراجع
- `docs/architecture/0042-licensing-consumer-integration.md` §4 — ربط العقدة.
- `docs/runbooks/licensing-installation.md` — التركيب والنتائج.
- `docs/runbooks/key-loss-and-reprovisioning.md` — إعادة التزويد بعد فقدان المفتاح.

# قاعدة الربط (Subject Binding)
- `payload.subject.id` هو الترميز المتعارف عليه لمفتاح العقدة العام (Ed25519 خام):

  ```
  subject.id = base64url(URL_SAFE_NO_PAD, node_public_key_bytes)   # 32 بايت تماماً
  ```

- المصدر: المفتاح العام الخام لمفتاح العقدة (`NodeKeyStore` → `derive_public_key`).
  **لا تشارك الشهادة أو سلسلة الثقة أو حالة الشهادة** في التحقق من الترخيص.
- التطابق **بالبايت بالضبط** (Byte-Exact) بين `decode(subject.id)` ومفتاح العقدة الحالي.
- الـ`subject.id` غير الصالح (ترميز خاطئ/وسادة/غير 32 بايت) يُرفض عند الاستيراد (Fail-Closed).

# النقطة المعمارية الأساسية
> **التوقيع الصحيح وحده لا يجعل الترخيص صالحاً للعقدة.**

`LICENSE_NOT_FOR_THIS_NODE` نتيجة **بعد التحقق من التوقيع** (Post-Signature): توقيع السلطة
صحيح والربط فشل. لا تُبلَّغ أبداً كـ`InvalidSignature`. هذا ثابت مُثبت عبر سيناريو
Node-A / Node-B الكامل.

# السلوك المثبت (A2)

## Node A — عقدة مرخّصة
- `import_license` → `outcome = "imported"`.
- `get_licensing_status` → `summary.license_count = 1`، حالة الترخيص `active`،
  `entitlements` تحتوي الصلاحيات المطلوبة.
- `verify_license` → `enforceable ≥ 1`.
- الإجراء المرخّص (مثال: `create_product` ضمن `core.stock`) **ينجح**.
- بعد إعادة تشغيل التطبيق (نفس قاعدة البيانات): الترخيص ما زال `active` والإجراء ما زال
  **ينجح** (التخزين المشتق يبقى عبر الإقلاع).

## Node B — عقدة غير مرخّصة (نفس anchor + نفس artifact)
- `import_license` بنفس الـartifact الذي نجح على A → `outcome = "not-for-this-node"`،
  الرسالة `license is not bound to this node`، ولا يُخزَّن أي شيء.
- `get_licensing_status` → `summary.license_count = 0`، `summary.gate_active = true`
  (البوابة نشطة رغم غياب الترخيص).
- الإجراء المرخّص نفسه → **مرفوض (Fail-Closed)** برسالة:
  `غير مرخّص: لا يوجد ترخيص سارٍ لهذه العملية`.

# الأثر التشغيلي
- مفتاح العقدة هو مرجع الربط: **أي تغيير في مفتاح العقدة يبطل التراخيص القائمة**.
- سيناريو إعادة التزويد: إعادة إقلاع الهوية دون اتصال يولّد زوج مفاتيح جديداً → يتغير
  `subject.id` → تصبح التراخيص القديمة `not-for-this-node`.
- **العلاج**: إعادة سك ترخيص جديد من سلطة التراخيص لمفتاح العقدة الجديد
  (راجع `key-loss-and-reprovisioning.md`).

# التحقق
1. على عقدة A: ثبّت anchor واستورد الترخيص وتحقق من `enforceable ≥ 1` ونفّذ إجراء مرخّصاً بنجاح.
2. على عقدة B: ثبّت **نفس** anchor وقدّم **نفس** الـartifact → تأكد من النتيجة
   `not-for-this-node` ورفض الإجراء المرخّص.
3. أعد تشغيل A وتأكد من استمرار نجاح الإجراء المرخّص.
