# RFC: Node Identity & Trust Architecture (هوية العقد وسلسلة الثقة)

**تاريخ:** 2026-08-04
**الحالة:** ✅ **Accepted** — اعتمد هذا الـ RFC بعد Architecture Review بتاريخ 2026-08-04
بناتيجة **ACCEPT — No blocking architectural findings** (المراجعة غطّت: نموذج الثقة، حالات
الهجرة، التوافق مع ADRs، قابلية تحقيق الـ Invariants، قابلية اختبار الخصائص الأمنية).
ملاحظة غير حاجبة بشأن `algorithm_version` أُغلقت بإضافة Invariant 10.
**النطاق:** Frozen contracts المتعلقة بالهوية والمصادقة والتوقيع واستيراد الحزم.
**المالك:** Architecture / Security

> **إعلان حوكمي:** اعتماد هذا الـ RFC يُفعّل الخطوة 3 من عملية `ARCHITECTURE_FREEZE.md`
> Section 4. المسودات الواردة في القسم 4 أدناه أصبحت **معتمدة للدمج** عبر تنفيذ B1:
> تعديل ADR-0003 + تحديث وثيقة التجميد + تحديث قواعد `check_arch` + تحديث `AGENTS.md` +
> إنشاء ADR جديد موثق. لا يبدأ التنفيذ البرمجي (B2–B6) إلا بعد اكتمال B1.

---

## 1. Problem Statement (بيان المشكلة)

### 1.1 الوضع الحالي للمشكلة الجوهرية

نظام GRPC هو تطبيق **offline-first** لإدارة عمليات المطاعم عبر عقد WILAYA وعقد UNIT
وعمليات إدارية (ADMIN). التحليل المنجز في المراحل C وD أثبت أن المشكلة المعمارية الجوهرية
ليست "المصادقة" (Authentication) وإنما **Distributed Mutable State**: حالة الهوية والثقة
موزعة على عقد متعددة، وكل عقدة تحتفظ بنسخة محلية، ولا توجد آلية حتمية لترتيب التحديثات
ومنع الرجوع للإصدارات الأقدم أو عزل مسؤولية النقل عن إدارة دورة حياة الهوية.

المشاكل القائمة اليوم:

1. **لا توجد هوية غير متماثلة.** توقيع الحزم هو HMAC-SHA256 بمفتاح مشترك يُدار عبر
   متغيرات البيئة (`GRPC_PACKAGE_SIGNING_KEY`)، مع قوائم trusted signers في البيئة
   (`src-tauri/src/infrastructure/security/mod.rs:103-236`). لا يوجد ربط توقيع بهوية عقدة
   ولا إثبات ملكية.
2. **`signature_version` غير مفعّل فعليًا.** الحقل موجود كـ `Option<u16>`
   (`src-tauri/src/application/sync/package_metadata.rs:22`) لكنه دائمًا `None` في كل مواقع
   التصدير — نقطة التوسّع جاهزة لكن غير مستخدمة.
3. **هوية ADMIN == كلمة مرور.** الدخول الإداري قائم على `password` مربوط بالعقدة عبر
   Argon2id + HMAC node binding، مع **admin افتراضي `admin/admin` مثبّت في الكود**
   (`src-tauri/src/application/services/user_service.rs:79`). تكلفة دوران كلمة المرور عبر
   50–300 وحدة مرتفعة، ولا يوجد نموذج حياة مفاتيح.
4. **هوية WILAYA بلا UUID.** هوية عقدة WILAYA هي `wilaya_code` (نصي) فقط؛
   `src-tauri/src/infrastructure/security/node_identity_provider.rs:25` يعيد `"WILAYA"` حرفيًا.
5. **مرجع الوحدة غير متجانس.** يُستخدم `units.id` و`units.code` و`settings.unit_name`
   بالتبادل في طبقات مختلفة (`guards.rs:100`، `source_node.rs:47`، `settings_service.rs:19-21`).
6. **لا يوجد فاصل بين مسؤوليات ثلاثة:** ترتيب النقل، إصدار الشهادات، وسلسلة الثقة —
   تُدار ضمنيًا عبر `package_id` idempotency وترقيم تسلسلي تشغيلي جزئي.

### 1.2 مبرر التغيير

- الحاجة إلى **هوية عقدة قابلة للتحقق** (إثبات ملكية) في التوقيع والاستيراد، بدل مفتاح
  مشترك في البيئة.
- الحاجة إلى **نموذج حياة مفاتيح صريح** (إصدار، تدوير، إبطال، استبدال) متوافق مع
  offline-first وبدون ساعة حائط في مسار التقييم.
- الحاجة إلى **فصل طبقات** يمنع اختلاط ترتيب الحزم بإصدار الشهادات وبالثقة، بما يحقق
  الحتمية (Freeze §2.5) وSingle Writer (§2.6) وFail-Closed (§2.2).

---

## 2. Current Behavior (السلوك الحالي)

السلوك الحالي الموثق في الكود:

### 2.1 الحسابات والمصادقة

- جدول `users` (`src-tauri/src/db/migrations/001_initial.sql:21-30`): `id UUID`,
  `username UNIQUE`, `password_hash`, `role (Admin|User|System)`, `node_id`, `deleted`.
- إنشاء admin الافتراضي عند أول تشغيل: `UserService::create_default_admin`
  (`src-tauri/src/application/services/user_service.rs:68-93`) — `admin/admin`، idempotent عبر
  `insert_user_if_absent` (`src-tauri/src/repositories/users.rs:108-123`).
- الدخول: `commands/auth.rs:21-171` — rate limit → Argon2 node-bound verify
  (`infrastructure/security/password_hash_provider.rs:30-41`) → `LoginPolicy`
  (`application/services/login_policy.rs:23-34`) → `CurrentSession` في الذاكرة
  (`domain/session.rs:33-63`).
- التفويض: `authorize_command` (`commands/guards.rs:65-126`) → `Principal` →
  `ResourceContext` (`application/authz/resource_context.rs:2-17`) → policies في
  `application/authz/policies/` (default-deny). WILAYA تتطلب Admin للقراءة المصدّقة
  (`policies/system.rs:52-65`).
- قواعد الحوكمة الحالية: Freeze §2.2 "Password operations confined to `commands/auth.rs`".

### 2.2 التوقيع والاستيراد

- توقيع الحزم: `HmacPackageSigner` (`infrastructure/sync/packages/signing/hmac_package_signer.rs`)
  فوق canonical JSON V2 (ADR-0009)؛ تسلسل البناء:
  serialize → canonical hash → `integrity_hash` → canonical sign → `signature` → age encrypt
  (`infrastructure/sync/packages/package_builder.rs:39-129`).
- التحقق عند الاستيراد: `verify_integrity` ثم `verify_signature`
  (`infrastructure/sync/packages/package_deserializer.rs:103-217`) — يلزم `source_node_id`
  ضمن trusted signers و`signing_key_id` ضمن accepted key ids وغير deprecated، ثم مقارنة
  constant-time.
- إدارة المفاتيح: env vars — `GRPC_ACTIVE_SIGNING_KEY_ID`, `GRPC_ACCEPTED_SIGNING_KEY_IDS`,
  `GRPC_DEPRECATED_SIGNING_KEY_IDS`, `GRPC_TRUSTED_SIGNER_IDS`, `GRPC_ENFORCE_TRUSTED_SIGNERS`
  (`infrastructure/security/mod.rs:150-236`). سياسات: ADR-0006 (rotation skeleton),
  ADR-0007 (deprecation window), ADR-0008 (trusted signer identity skeleton).
- الاستيراد: `run_import_pipeline` (`commands/import_export.rs:851-1019`) — auth + touch +
  file validation + decrypt + idempotency عبر `package_id` (`ImportedPackageRegistry`) +
  audit (`ImportStarted`/`ImportRejected`/`SyncPackageImported`) + reproducibility record.
- حماية إعادة الإرسال: `package_id` idempotency، وبالنسبة للمسار التشغيلي فقط
  `incoming_sequence = last_applied + 1` (`application/sync_integrity/sequencing.rs:137-170`).
  **لا يوجد أي `generation` أو `nonce` على غلاف الحزمة.**

### 2.3 هوية العقدة

- `current_node_id()` يعيد `settings.unit_name` لعقد UNIT أو `"WILAYA"` حرفيًا
  (`infrastructure/security/node_identity_provider.rs:5-27`).
- `source_node_id` للتصدير: WILAYA → `wilaya_code`، UNIT → `units.id` عند قابلية الحل
  (`infrastructure/sync/source_node.rs:8-19`).
- لا توجد مفاتيح عامة ولا شهادات ولا أزواج مفاتيح للعقد في أي مكان في `src-tauri/`.

### 2.4 العقود المجمدة المتأثرة (مرجعية)

- **Freeze §2.2 Authorization Model** — fail-closed، authz منظم، حصر كلمات المرور في
  `commands/auth.rs`.
- **Freeze §2.5 Reproducibility & Determinism** — لا ساعة حائط في مسارات التقييم
  (بما فيها sync_import/sync_integrity)؛ لا عشوائية؛ حزم متزامنة حتمية.
- **Freeze §2.6 SQLite Topology** — single-writer، لا async، لا threads، WAL.
- **Freeze §2.7 Sync Protocol** — package-only transport (ADR-0010)، Canonical JSON V2
  (ADR-0009)، تشفير على مستويين (ADR-0039): `age::x25519` للمفاتيح المدارة على العقدة
  و`age::scrypt` حصريًا لمفتاح المشغّل المحمول `.adminkey`،
  streaming encryption (Rules 29, 31)، replay قبل أي كتابة (Rules 83-84)،
  بدون transactions متداخلة (Rule 89).
- **ADR-0003** — يؤجل التواقيع غير المتماثلة صراحةً ("خارج النطاق": PKI، التواقيع
  غير المتماثلة، سياسات تدوير/إبطال المفاتيح).
- **ADR-0004** — تغييرات البروتوكول (الشكل المتسلسل، حقول `SyncPackageMetadata`،
  نافذة التوافق) هي Breaking Changes تتطلب المراجعة الإلزامية.

---

## 3. Proposed Change (التغيير المقترح)

### 3.0 ملخص القرارات المثبتة في هذه الجلسة

القرارات التالية مثبتة وتُعامل كأساس لهذا الاقتراح (تم اعتمادها خلال مراجعات
المراحل C/D/E):

1. **Option C مباشرة**: هوية Ed25519 (وليس Option B القائمة على كلمة المرور/HMAC).
2. **Administrator هوية (Identity) لا اعتماد (Credential)**:
   `login == إثبات هوية` وليس `login == password`.
3. **Credential ID (UUID)** ثابت مدى الحياة + **Generation (u64)** — بلا ساعة حائط.
4. **مرساة ثقة واحدة** وسلسلة ثقة موحدة من أول لحظة:
   `Authority Root → WILAYA → UNIT/ADMIN`.
5. **لا توجد أي شهادة ADMIN ذاتية التوقيع إطلاقًا.**
6. **`EXPIRED` حالة صريحة تصدرها السلطة** وليست نتيجة مقارنة زمنية محلية.
7. التحدي (Challenge) مبني على `session_id` + `nonce` فقط، **بدون timestamps**.
8. **فصل ثلاث طبقات مستقلات**:
   - Transport Ordering → `(issuer_identity_id, package_sequence)`
   - Credential Versioning → `(credential_id, generation)`
   - Trust Chain → `Root → WILAYA → UNIT/ADMIN`

### 3.1 النطاق والحدود (Scope & Boundaries)

**يغيّر هذا الـ RFC هذه المجالات فقط:**

| المجال | النطاق |
|--------|--------|
| Identity | نموذج هوية العقد (WILAYA/UNIT/ADMIN)، Identity Store، مراجع UUID |
| Trust | سلسلة الثقة Root→WILAYA→UNIT/ADMIN، الشهادات، الإصدار والإبطال |
| Authentication | بروتوكول Challenge–Response؛ استبدال مسار bootstrap الافتراضي |
| Package Signing | `signature_version = 2` (Ed25519)؛ حزم trust/registry؛ الحارسان |

**لا يغيّر هذا الـ RFC:**

| المجال | السبب |
|--------|-------|
| Authorization | يبقى `authorize_command`/`CurrentSession` كما هو (Freeze §2.2) |
| Session Model | `CurrentSession` بالذاكرة دون تغيير؛ يُضاف مسار مصادقة محصور فقط |
| Domain Model / Business Logic | لا تغيير على المجال المحاسبي/المخزني/المالي |
| Sync semantics | تبقى كما هي؛ التغيير محصور في حزم الثقة ومسار التحقق |

> أي تغيير خارج هذه الحدود يُعتبر خارج نطاق هذا الـ RFC ويستوجب RFC مستقلًا.

### 3.2 Identity Invariants (قوانين الهوية الثابتة)

قواعد إلزامية (عقد معماري) لا يجوز كسرها دون RFC مستقل — تُصاغ بصيغة معيارية
(MUST / MUST NOT):

1. **Identity ID immutable** — `identity_id` MUST remain immutable for the lifetime of the
   identity; it MUST NOT change across rotation or re-issue.
2. **Credential ID immutable across rotation** — `credential_id` MUST NOT change on Rotate;
   it MUST change only on Re-Issue.
3. **Generation strictly monotonic** — `generation` MUST be strictly monotonic within the
   same `credential_id`; it MUST NOT regress.
4. **One active credential per identity** — each identity MUST have at most one ACTIVE
   credential at any moment.
5. **Root never signs ADMIN directly** — Root MUST sign only WILAYA identities; it MUST NOT
   sign UNIT identities, MUST NOT sign ADMIN identities, and MUST NOT sign packages.
6. **Package ordering independent from credential ordering** — Transport Guard MUST NOT depend
   on generation; Credential Guard MUST NOT depend on package ordering.
7. **No self-signed identities** — no certificate in the trust model MAY be self-signed,
   including during bootstrap.
8. **No wall-clock in evaluation** — no wall-clock MAY be used in any accept/reject path
   (توافق Freeze §2.5).
9. **Identity Store is the sole source of truth for identity state** — authentication,
   package verification, and trust evaluation MUST derive identity state exclusively from the
   Identity Store.
10. **algorithm_version immutable per credential** — `algorithm_version` MUST be immutable for
    an issued credential and MAY change only through a Rotate or Re-Issue operation; it MUST
    NOT change in place for the same `credential_id`.

### 3.3 النموذج (Identity Model)

#### 3.3.1 الهوية والشهادة

```
IdentityCertificate {
    identity_id: UUID            // ثابت مدى الحياة، مرجع كل العمليات
    subject_type: WILAYA | UNIT | ADMIN
    subject_id: UUID             // مرجع ثابت للكيان؛ الأسماء/الرموز Metadata فقط
    issuer_identity_id: UUID     // مُصدر الشهادة (معرف ثابت، لا اسم ولا كود)
    credential_id: UUID          // معرف الاعتماد الحالي لهذه الهوية
    generation: u64              // إصدار الاعتماد الحالي
    status: ACTIVE | REVOKED | SUPERSEDED | EXPIRED
    public_key: Ed25519
    not_after: Option<...>       // بيانات وصفية/إرشادية فقط — لا تُقيَّم في مسار القبول
    algorithm_version: u16       // signature_version الذي صدرت به الشهادة
    signature: Ed25519Signature  // من المُصدر
}
```

- **subject_id هو المرجع الوحيد** (إزالة `username`/`wilaya_code`/`unit_name` كمراجع هوية).
- الأسماء والرموز (`username`, `wilaya_code`, `unit_name`, `units.code`) تبقى **Metadata**.
- **WILAYA تُمنح UUID ثابتًا** عبر Identity Store؛ يبقى `wilaya_code` كودًا وبيانات وصفية.
- مرجع وحدة UNIT **يُوحَّد على `units.id`** في كل الطبقات (إزالة التناقض الحالي).

#### 3.3.2 Identity Store

مكوّن مستقل (جداول + repository + خادم مجال) يحوّل ADMIN/UNIT/WILAYA إلى **كيانات من
نفس الفئة**، منفصلًا عن جدول `users` (الذي يبقى كمسار إرث خلال نافذة الهجرة فقط).

لكل هوية مخزّنة:

```
IdentityState {
    credential_id: UUID
    generation: u64
    issuer_identity_id: UUID
    status: ACTIVE | REVOKED | SUPERSEDED | EXPIRED
    package_sequence: u64       // أثر تدقيق: آخر حزمة أدخلت هذا التغيير
}
```

> **توضيح المسؤوليات:** `package_sequence` داخل `IdentityState` هو **أثر تدقيق** (أي حزمة
> غيّرت هذه الهوية). **سجل الترتيب الفعلي** للنقل (عدّاد لكل مُصدر: آخر حزمة طُبّقت)
> كيان منفصل ضمن `run_import_pipeline` — ولا يجب الخلط بينهما.

#### 3.3.3 دورة حياة الاعتماد (Credential Lifecycle)

- **Rotate** = نفس `credential_id` + `generation++` + مفتاح عام جديد.
- **Re-Issue** = `credential_id` جديد + `generation = 1`.
- الحدثان **مختلفان** ويُوثّقان كحدثين منفصلين في سجل التدقيق.
- الحالات: `ACTIVE → REVOKED | SUPERSEDED | EXPIRED` (حالات نهائية صريحة).
- `not_after` إرشادي: يُستخدم للأدوات والتنبيهات فقط، **لا** يُقيَّم في مسار القبول.

### 3.4 فصل طبقات الترتيب (الحارسان المستقلان)

#### 3.4.1 Transport Guard — `(issuer_identity_id, package_sequence)`

- **النطاق:** لكل مُصدِر (Issuer Identity).
- **القيمة:** `package_sequence` تزداد مع كل حزمة يصدرها المُصدِر.
- **الاستخدام:** داخل `run_import_pipeline` فقط.
- **الحارس:**
  ```
  expected = last_applied_sequence[issuer] + 1
  if incoming.sequence != expected: reject / defer
  ```
- **يحل:** out-of-order، duplicate، replay، missing package. لا علاقة له بالمحتوى.

- **سجل المُنتِج (Commit ④b):** على جانب المُنتِج، `sync_issuer_sequence_state` يتتبّع
  آخر تسلسل مصدَّر لكل هوية عقدة محلية (`issuer_identity_id` — ليس `credential_id`)؛
  استمرارية الترتيب النقلي عبر دوران الشهادات (القيود أعلاه §3.4.4). تخصيص التسلسل
  وكتابته منفصلان: `begin_export` يقرأ فقط ويعيد رمزًا معلّقًا، ويتقدّم السجل
  حصريًا عبر `commit()` بعد نجاح بناء/كتابة الحزمة (advance-on-success) — الفشل لا
  يحرق تسلسلًا، وإعادة المحاولة تعيد نفس الرقم. `IdentitySignedExportService` هو
  المدخل الوحيد لتصدير V2 (products/daily_report/monthly_summary/stock_movements).

#### 3.4.2 Credential Guard — `(credential_id, generation)`

- **النطاق:** لكل شهادة.
- **الحارس:**
  ```
  incoming.generation > stored_generation(credential_id)
  ```
- يرفض `(X,2) → (X,1)` مهما كان ترتيب الحزم. مثال:
  ```
  Package 400: Credential X Gen 9
  Package 401: Registry update
  Package 402: Credential X Gen 8   ← مرفوض (رغم صحة الترتيب النقلي)
  ```

#### 3.4.3 لماذا الفصل؟

`global_generation` يخلط Transport Ordering بـ Credential Lifecycle. مثال يوضح الفصل:

```
Package 100 Registry
Package 101 Trust (Credential A Gen=7)
Package 102 Inventory
Package 103 Trust (Credential B Gen=3)
```

- Transport: `100, 101, 102, 103` بترتيب متسق.
- Credential: A=7 وB=3 مستقلان تمامًا.
- لا يوجد عداد عالمي، وبالتالي لا ترابط بين الهويات ولا تعقيد في الاستعادة الجزئية.

#### 3.4.4 المستويات الثلاثة وقيود الاستقلال (Three Levels & Independence)

المستويات الثلاثة مرتبة بوضوح:

```
Transport Ordering   → (issuer_identity_id, package_sequence)
        ↓
Trust Distribution  → حزم trust/registry عبر run_import_pipeline
        ↓
Credential Lifecycle → (credential_id, generation)
```

قيود استقلال صريحة (تُفرض في التنفيذ وتُفحص في المراجعة):

- **Credential Guard MUST NOT depend on package ordering** — قبول/رفض الشهادة لا يُبنى على
  ترتيب الحزم، بل على `(credential_id, generation)` فقط.
- **Transport Guard MUST NOT depend on generation** — قبول/رفض الحزمة لا يُبنى على إصدارات
  الشهادات، بل على `(issuer_identity_id, package_sequence)` فقط.
- **Trust Package هو الناقل الوحيد لتوزيع الثقة** — لا قناة جانبية لنشر الشهادات أو الإبطالات.

> القيود أعلاه تطبيق تنفيذي لـ **Invariant 6** — المبدأ المعماري المرجعي.

### 3.5 سلسلة الثقة (Trust Chain)

```
Authority Root (offline, escrow — يوقّع WILAYA فقط)
        │
        ▼
WILAYA (يوقّع UNIT + ADMIN + حزم trust/registry)
        ├── UNIT
        └── ADMIN
```

- **Root يوقّع فقط هويات WILAYA.** لا يوقّع UNIT، لا يوقّع ADMIN، لا يوقّع الحزم.
- العقدة تحمل **مفتاح Root العام كمرساة ثقة** فقط؛ مفتاح Root الخاص لا يصل لأي عقدة.
- شهادة WILAYA الموقّعة من Root تُزوَّد عند **التهيئة الفيزيائية** (provisioning) —
  الطريقة الوحيدة المتسقة مع offline-first.

> **Normative rule:** Root signs only WILAYA identities. Root MUST NOT sign UNIT identities,
> MUST NOT sign ADMIN identities, and MUST NOT sign packages. Root is not an operational CA.
> (تطبيق تنفيذي لـ **Invariant 5** — القاعدة المجردة المرجعية.)

#### 3.5.1 Security Assumptions (افتراضات النموذج الأمني)

افتراضات التصميم التي بُني عليها هذا الـ RFC — كل افتراض مربوط بالخاصية الأمنية التي
يضمنها النظام (مرجع لمراجعة التنفيذ لاحقًا):

| الافتراض | الخاصية الأمنية المطلوبة |
|----------|--------------------------|
| Authority Root private key is offline (escrow) | لا يُستخدم في التشغيل اليومي؛ يوقّع WILAYA فقط عند التهيئة الفيزيائية |
| Compromise of a WILAYA private key is within the threat model | MUST NOT compromise identities issued by other WILAYA deployments or the Authority Root |
| Physical capture of a UNIT is within the threat model | MUST NOT compromise identities other than that UNIT |
| Offline operation is mandatory | لا اتصال شبكة في أي مسار تقييم؛ توزيع الثقة حصريًا عبر الحزم |
| No trusted online service exists | لا مرجع إنترنت للتحقق؛ التقييم محلي من Identity Store |
| Replay is prevented by Transport Guard | يرفض إعادة التسليم عبر `(issuer_identity_id, package_sequence)` |
| Rollback is prevented by Credential Guard | يرفض الرجوع لإصدار أقدم عبر `(credential_id, generation)` |

> هذه الافتراضات توثّق الشروط التي صُدّق عليها التصميم — لا توسّع نطاق الـ RFC (بند §3.1).

### 3.6 Bootstrap (إنشاء الهوية الأول)

التسلسل المعتمد (استبدال `admin/admin`):

```
Authority Root
        │
        ▼
Bootstrap WILAYA Identity    (موقّعة من Root، تُثبَّت أولًا)
        │
        ▼
Bootstrap ADMIN Identity     (تصدرها WILAYA فورًا — لا ذاتية توقيع)
```

- **لا توجد أي شهادة ADMIN ذاتية التوقيع إطلاقًا** — مرساة ثقة واحدة من أول لحظة.
- عند bootstrap تُولَّد مفاتيح ADMIN عند المُصدر (issuer-generates) — لا يوجد جهاز مشغّل
  بعد في أول تشغيل؛ الناتج ملف `.adminkey` يُسلَّم للمشغّل.

#### 3.6.1 Recovery Mode (خارج نموذج الثقة)

إن لزم حساب طوارئ قبل تهيئة WILAYA، يُعامل كـ **Recovery Mode منفصل خارج نموذج الثقة**:
- ليس جزءًا من سلسلة الشهادات.
- حارس منفصل، مسجّل تدقيقيًا، مقيّد زمنيًا.
- لا يُدرج كاستثناء في السلسلة بل ككيان حوكمي مستقل.

> **Normative rule:** Recovery Mode is operational recovery and is NOT part of the identity
> trust chain. It issues no certificates and cannot be chained to or from any identity.

### 3.7 بروتوكول Challenge–Response

- رسالة تحدي **ثابتة البنية** (Canonical Representation تُوقَّع):

```
Challenge {
    protocol_version
    session_id          // UUID من الخادم — بلا timestamps
    node_identity_id
    nonce               // 32 bytes
    challenge_version
}
```

- يوقّع ADMIN التمثيل القانوني للرسالة، وتتحقق العقدة بـ constant-time (`ct_eq`).
- المزايا: لا ساعة حائط، منع إعادة استخدام التوقيع بين الجلسات، ربط التوقيع بالعقدة
  المستهدفة، قابلية تطوير البروتوكول عبر `challenge_version`.
- **CurrentSession و`authorize_command` لا يتغيران** — يظلان كما هما؛ يُضاف مسار مصادقة
  جديد (Challenge–Response) محصور، وتبقى كلمات المرور (مسار الإرث) محصورة في
  `commands/auth.rs` خلال نافذة الهجرة.

### 3.8 ملف `.adminkey` (محمول، مكتفٍ ذاتيًا)

الملف يحتوي، لا المفتاح فقط:

```
.adminkey {
    encrypted_private_key   // Ed25519 secret key — age::scrypt بعبارة مرور operator محلية (ADR-0039)
    certificate             // الشهادة الحالية (تشمل توقيع المُصدر)
    format_version          // نسخة بنية الملف (.adminkey schema) — مستقلة عن algorithm_version
    algorithm_version       // هوية Algorithm Profile — معرف مستقل (حاليًا = 2 بالاتفاق لا بالدلالة)
}
```

- `credential_id` و`generation` تُقرأ من الشهادة المضمّنة (مصدر واحد للحقيقة — P2) ولا
  تُكرَّر كحقول مستقلة.
- **نقل ملفي كامل**: الملف لا يعتمد على أي Secret خاص بالعقدة (`GRPC_APP_KEY`، هوية
  العقدة، أي سر محلي)؛ يُفتح بعبارة مرور operator فقط — قابل للحمل بين الأجهزة.
- فضاءات الإصدار الثلاثة (`format_version` / `algorithm_version` / `signature_version`)
  **مستقلة** ويُوثَّق تساويها الحالي كاتفاق تنظيمي وليس علاقة دلالية (ADR-0039 §4).

### 3.9 أنواع الحزم الجديدة

فصل أنظف للاستقبال المستقبلي:

| الحزمة | المحتوى |
|--------|---------|
| **Trust Package** (kind = `trust`) | certificates + revocations + ترقيم/تسلسل |
| **Registry Package** (kind = `registry`) | حالة الأسطول (fleet state) |

- كلاهما عبر `run_import_pipeline` مع kind جديد صريح لكل منهما.
- لا يُحمَّل registry_snapshot كامل داخل كل حزمة ثقة.

### 3.10 الهجرة إلى `signature_version = 2` (Ed25519)

- `signature_version = 2` = توقيع Ed25519 بهوية العقدة.
- يبقى HMAC (version 1 / الحزم الإرثية) **قابلًا للقراءة** خلال نافذة إهمال بنمط
  ADR-0007 (deprecation window). منذ Commit ④b، **حزم الإنتاج الأربع**
  (products/daily_report/monthly_summary/stock_movements) تُصدَّر حصريًا
  `signature_version = 2` عبر `IdentitySignedExportService` — بدون أي مسار تراجع
  HMAC: عقدة غير مجهَّزة تفشل مغلقةً ولا تصدر حزمة V2.
- **استثناء Bootstrap الوحيد:** `.unit` تبقى موقّعة HMAC (بدون `signature_version`)
  — استثناء دائم. السبب: مسار استيراد `.unit` يتجاوز `run_import_pipeline` (لا يمر
  على Transport Guard) ولا تملك عقدة UNIT مرساة ثقة/هوية عند التزويد — دائرة
  «تحتاج ثقة ← تحتاج `.unit`». لا تُصدَّر عبر `IdentitySignedExportService`.
- Ed25519 حتمي (RFC 8032) — لا أثر على حتمية Freeze §2.5.
- Canonical JSON V2 (ADR-0009) يبقى؛ يوقّع Ed25519 نفس البايتات القانونية.
- **التشفير على مستويين (ADR-0039):** `age::x25519` للأسرار المدارة على العقدة
  (مفاتيح WILAYA/UNIT — `node_key_store`)؛ `age::scrypt` حصريًا لمفتاح المشغّل المحمول
  `.adminkey`؛ التوقيع عبر Ed25519 — لا تغيير في تشفير الحزم.

### 3.11 البدائل المدروسة (Alternatives Considered)

- **Option B (هوية كلمة مرور / HMAC):** رُفض — عبء تشغيلي مرتفع لدوران كلمات المرور عبر
  50–300 وحدة، ولا يقدم إثبات ملكية.
- **global_generation (عداد عالمي):** رُفض — يخلط Transport Ordering بـ Credential
  Lifecycle، يربط كل الهويات، ويعقّد الاستعادة الجزئية.
- **ADMIN ذاتية التوقيع (self-signed bootstrap):** رُفض — يضيف مرساة ثقة ثانية مؤقتة
  ويكسر نموذج الثقة الموحد.
- **Bootstrap بهوية WILAYA أولًا ثم إصدار ADMIN (المعتمد):** يبقي مرساة واحدة منذ
  أول لحظة.

### 3.12 قرارات تُفتح أثناء مراجعة الـ RFC

1. موقع توليد زوج مفاتيح WILAYA (عقدة التركيب/provisioning أم على العقدة نفسها).
2. طول نافذة إهمال HMAC وجدول `users` (المدة الدقيقة للانتهاء).
3. مالك حوكمة Recovery Mode وحدوده الزمنية.

#### قرارات مقرّرة (B6-A)

| القرار | الحكم المعتمد |
|--------|---------------|
| نقطة قطع التحول للمصادقة (D1) | التحول يعتمد على **وجود هوية ADMIN نشطة** (ACTIVE ADMIN certificate + `.adminkey`)، وليس على اكتمال تنظيف كلمة المرور الإرثية. `issue_first_admin_key` على عقدة إرثية ينشئ هوية ADMIN نشطة → جميع متطلبات المصادقة الجديدة قائمة، والاحتفاظ بكلمة المرور لمجرد وجود هاش ليس ذا معنى. |
| طريقة إنهاء نافذة الإهمال (بند 2) | نافذة مسار كلمة المرور تنتهي عند B6-B: `GRPC_LEGACY_AUTH` (Introduced B6-A) يُحذف نهائيًا في B6-B ولا يبقى متغير بيئة دائمًا. **مكتمل في Commit ③**: حُذف متغير البيئة والـ override من `IdentityAuthenticationPolicy`، والبوابة أصبحت دائمة — مسار كلمة المرور يُبقي مفتوحًا فقط عند غياب هوية ADMIN نشطة (مستخدمي UNIT المحليون من `.unit` package والعقد غير المجهَّزة)، بينما عقد WILAYA ذات هوية ADMIN نشطة تُوجَّه حصريًا إلى Challenge–Response. جدول `users` يُقرأ خلال النافذة. |
| آلية مصادقة العقدة البديلة | تحليل/تسليم مفاتيح UNIT عبر مسار CSR موحّد (`generate_identity_request`/`sign_identity_request`/`finalize_identity_provision` — D2) في Commit ②. |
| تسليم شهادة WILAYA إلى عقدة UNIT (D2) | **خطوتان صارمتان — لا تُضمَّن أبدًا** في ملف إمداد UNIT. الترتيب: UNIT تصدّر `generate_identity_request()` → WILAYA توقّع `sign_unit_identity_request()` → UNIT تُثبّت/تحدّث شهادة WILAYA النشطة كمرساة ثقة محلية عبر `install_wilaya_certificate()` (خدمة مستقلة، لا تقرأ `NodeKeyStore`، لا تفحص حالة UNIT، لا تمسّ `IdentityBootstrapState`؛ قابلة لإعادة الاستخدام في B7 rotation) → ثم `finalize_unit_provision()`. |
| مصدر `subject_id` لـ UNIT (D2) | يُحلّ **محليًا** على عقدة UNIT: `settings.unit_code → units.get_unit_by_code → units.id` عند توليد الـ CSR. WILAYA لا تفرض/تعيد ربطه — تتحقق فقط fail-closed أن `CSR.subject_id` يطابق صف `units` معروفًا (`units.get_unit`). |
| معرّف المُصدِر عند إنهاء إمداد UNIT (D2) | يُحلّ حصريًا عبر `signed_cert.issuer_identity_id → IdentityStore.get_by_identity_id(...)` ثم التحقق (موجود + `ACTIVE` + `subject_type == WILAYA`) — وليس عبر `get_active_by_subject_type(Wilaya)`. |
| تعريف الـ idempotency لمرساة الثقة | **نفس الشهادة حرفيًا** (`is_identical_to`): نفس `identity_id` مع `credential_id` مختلف → Fail-Closed وليس `AlreadyInstalled`. |
| حالة bootstrap لعقدة UNIT | سلسلة UNIT مستقلة: `Uninitialized → UnitWaitingForCertificate → UnitActive`؛ لا ADMIN على عقدة UNIT إطلاقًا. `IdentityBootstrapStatusService::compute()` تتفرّع حسب `node_type` (نص خام `"UNIT"` فقط) مع **افتراض WILAYA** — بذرة `UNCONFIGURED` ليست عقدة UNIT. |

#### توضيح B7: تسجيل دوران UNIT كـ **Issuer Local State** (وليس قناة توزيع)

عندما توقّع WILAYA شهادة دوران UNIT عبر `sign_unit_rotation_request`، تُسجَّل النسخة
الموقَّعة محليًا في Identity Store الخاص بـ WILAYA (افتراضيًا `SUSPENDED`، تحل محل أي
صف `ACTIVE` سابق بنفس `subject_type`/`subject_id`). هذا التسجيل هو **حالة محلية للمُصدِر**
(سجل تتبّع إصدار) وليس **قناة توزيع**: قناة التوزيع الوحيدة بين العقد تبقى Trust Package
(§3.4.4). حارس الاعتماد على جانب WILAYA هو **best-effort** — إذا تقرير الحارس
Rollback/RejectZero (نزاع توليد) تُتخطَّى المعالجة المحلية مع تحذير ولا يتعطّل التوقيع؛
القرار الأمني النهائي يبقى عند `CredentialGuard` + `finalize_unit_rotation` على عقدة UNIT
(fail-closed). الشهادة الموقَّعة تُسلَّم للمشغّل عبر ملف (تدفّق ملفات، لا IPC تصدير حزم).

---

## 4. Impact Analysis (تحليل الأثر — العقود المجمدة المتأثرة)

> ⚠️ **حالة القسم: مسودات معتمدة للدمج.** النصوص أدناه كانت مسودات وقت الـ RFC؛ وبعد
> اعتماده أصبحت **مقرّرة للدمج عبر B1** وفق خطوة 3 من ARCHITECTURE_FREEZE §4
> (تعديل ADR-0003 + تحديث Freeze + تحديث `check_arch` + تحديث `AGENTS.md`).

### 4.1 العقود المجمدة المتأثرة (Freeze Section 2)

| البند | الأثر |
|-------|-------|
| **§2.1** | لا تغيير على الطبقات؛ يُضاف مكوّن Identity Store في طبقة المجال/repositories وفق الخريطة الحالية. |
| **§2.2 Authorization** | يبقى fail-closed؛ `CurrentSession`/`authorize_command` دون تغيير؛ يُضاف مسار Challenge–Response محصور؛ بقاء حصر كلمات المرور في `commands/auth.rs` خلال النافذة. |
| **§2.5 Determinism** | متوافق: الحارسان بلا ساعة حائط؛ Ed25519 حتمي؛ `EXPIRED` لا يُشتق من ساعة؛ لا عشوائية في التقييم. |
| **§2.6 SQLite** | متوافق: جداول جديدة (additive) بدون DDL مدمر على `users` أو `audit_log`؛ single-writer يبقى. |
| **§2.7 Sync Protocol** | يتطلب تعديلًا (انظر 4.3): إضافة أنواع حزم + حقول غلاف + توقيع Ed25519؛ يبقى package-only، Canonical JSON V2، age::x25519 للتشفير. |
| **§2.8** | قواعد جديدة لـ `check_arch` (انظر 4.5)؛ منع admin الافتراضي في production. |

### 4.2 أثر على ADR-0003 — [Draft — مسودة غير معتمدة]

المقترح: إزالة "التواقيع غير المتماثلة" من "خارج النطاق" والسماح بالتوقيع Ed25519.
يبقى PKI/X.509 خارج النطاق.

```
# تعديل مقترح على ADR-0003 (مسودة — يُدمج بعد اعتماد RFC)
- خارج النطاق يتحول إلى:
  * البنية التحتية للمفاتيح العامة PKI/X.509 (تبقى خارج النطاق).
  * التواقيع غير المتماثلة: تُسمح بـ Ed25519 فقط (RFC 8032) وفق نموذج الهوية المعتمد.
- إضافة: التوقيع اختياري تاريخيًا؛ جديد الحزم يلزمها إمّا integrity فقط (V1)
  أو Ed25519 (signature_version = 2).
```

### 4.3 أثر على ARCHITECTURE_FREEZE §2.7 — [Draft — مسودة غير معتمدة]

```
# تعديل مقترح على Freeze §2.7 (مسودة — يُدمج بعد اعتماد RFC)
- التشفير على مستويين (ADR-0039):
  * الأسرار المدارة على العقدة: age::x25519 فقط (مفاتيح WILAYA/UNIT).
  * مفتاح المشغّل المحمول `.adminkey`: age::scrypt فقط (Rule 38 كما عُدّلت — حصري
    في `infrastructure/identity/adminkey_provider.rs`).
  * التوقيع/المصادقة: Ed25519 بهوية العقدة (signature_version = 2).
  * (المفتاح المشترك HMAC يبقى لقراءة حزم V1 خلال نافذة الإهمال فقط.)
- إضافة بند: Trust/Registry packages عبر run_import_pipeline
  مع حارس ترتيب نقل (per-issuer package_sequence) وحارس إصدار اعتماد
  (credential_id, generation) — Fail-Closed وبدون ساعة حائط.
```

### 4.4 أثر على ADR-0004 (Breaking Changes)

- إضافة أنواع حزم (`trust`, `registry`) وحقول غلاف جديدة (`package_sequence`,
  حقول هوية/توقيع) هي **Public Contract Change** — تُلزم: مراجعة التوافق، تحديث
  Snapshot Contract، اختبارات سلوك استيراد (نجاح + رفض صريح)، وتحديث ADR.
- **لا تُنفَّذ أي من هذه التغييرات قبل اعتماد الـ RFC.**

### 4.5 قواعد `check_arch` الجديدة — [Draft — مسودة غير معتمدة]

```
# قواعد مقترحة (مسودة — تُدمج بعد اعتماد RFC)
- Rule N: توقيع الحزم الجديدة يجب أن يُنسب لهوية عقدة (Ed25519) — لا env HMAC للحزم الجديدة.
- Rule N: ManageUnits/عمليات إدارة الوحدات تتطلب حارس node-type (WILAYA فقط) عبر authz.
- Rule N: منع admin الافتراضي (admin/admin) في production.
- Rule N: signature_version إلزامي للحزم الجديدة.
```

### 4.6 أثر على AGENTS.md — [Draft — مسودة غير معتمدة]

```
# تعديل مقترح على AGENTS.md (مسودة — يُدمج بعد اعتماد RFC)
- إضافة: هوية العقدة = Ed25519 (signature_version=2)؛ التشفير يبقى age::x25519 فقط.
- إضافة: مكوّن Identity Store (جداول + repository) — ADMIN/UNIT/WILAYA من نفس الفئة.
- إضافة: Recovery Mode خارج نموذج الثقة — حارس منفصل ومحدد زمنيًا.
```

### 4.7 عدم التأثر

- ADR-0002 (Sync Package Boundary) وADR-0010 (package-only transport): يبقيان.
- ADR-0005/0009 (Canonical JSON): يبقيان؛ Ed25519 يوقّع نفس البايتات.
- ADR-0015 (Streaming encryption), ADR-0019 (age-only): يبقيان كما هما — تُضاف إليهما
  ADR-0039 (tiered: x25519 node / scrypt `.adminkey` فقط).
- ADR-0006/0007/0008: يُعاد ضبط دورهما — يبقيان لسياسة مفاتيح HMAC الإرثية خلال
  نافذة الإهمال فقط.

---

## 5. Migration Plan (خطة الهجرة)

التسلسل الحوكمي المعتمد (Proposal → Review → Approval → ADR Update → Implementation):

- **B0 (هذا المستند):** إنشاء الـ RFC كاقتراح + المسودات المرفقة.
- **B1 (بعد الاعتماد):** إنشاء ADR جديد + تعديل ADR-0003 + تحديث Freeze §2.7 +
  قواعد `check_arch` + `AGENTS.md` + AGENTS.md. — لا يُنفَّذ قبل الاعتماد.
- **B2:** مخطط Identity Store (جداول إضافية فقط) + إسناد UUID ثابت لـ WILAYA +
  توحيد مرجع الوحدة على `units.id`.
- **B3:** توليد مفاتيح Ed25519 + بروتوكول Challenge–Response + ملف `.adminkey`.
- **B4:** حزم Trust/Registry + الحارسان (Transport + Credential) داخل
  `run_import_pipeline`.
- **B5:** استبدال bootstrap admin (نموذج WILAYA→ADMIN) + بدء نافذة إهمال
  `users`/كلمات المرور.
- **B6-A:** إغلاق بوابة كلمة المرور عند وجود هوية ADMIN نشطة (Authentication
  Cutover عبر `IdentityAuthenticationPolicy`).
- **B6-B (أجزاء مستقلة):** Commit ③ — إزالة `GRPC_LEGACY_AUTH`/`change_password`
  وإغلاق نافذة كلمة المرور ببوابة دائمة (Authentication Final Cutover). Commit ④ —
  قلب الافتراضي إلى `signature_version = 2` (Ed25519)؛ HMAC يبقى للقراءة فقط.
  Commit ④ يُسلَّم على مرحلتين: ④a (البنية التحتية السلوكية-المحايدة) و④b
  (القطع السلوكي B6-B عبر `IdentitySignedExportService`).

### حالة التنفيذ (Progress Log)

| الخطوة | الحالة | تاريخ الإغلاق | الأدلة |
|--------|--------|---------------|--------|
| B0 (RFC) | ✅ مكتمل | 2026-08-04 | اعتماد الـ RFC بعد Architecture Review |
| B1 (الحوكمة) | ✅ مكتمل | 2026-08-04 | ADR-0038 + ADR-0039 + تعديل Freeze §2.7 + قواعد `check_arch` (Rule 38/126/127/128/129) + تحديث AGENTS.md |
| B2 (Identity Store) | ✅ مكتمل | 2026-08-04 | `identity_store` table + repository + `003_certificate_signature.sql` + `CurrentSession::new_with_session_id` |
| B3 (Ed25519 + Challenge–Response + `.adminkey`) | ✅ مكتمل | 2026-08-05 | 21 اختبار domain + 6 اختبار repository + 7 اختبار infrastructure + 7 اختبارات تكامل `identity_trust_tests` + بوابة الواجهة الأمامية كاملة (`check`/Vitest/`tauri build`) + `check:arch` صفر تحذيرات |
| B4 (Trust/Registry packages + الحارسان) | ✅ مكتمل | 2026-08-05 | 16 اختبار تكامل `sync_trust_registry_import_tests` + أمرا `import_trust_package`/`import_registry_package` + authz WILAYA-admin + `AuditAction` جديدان + البوابة كاملة (`cargo test`/clippy `-D warnings`/`check:arch`/svelte-check) |
| B5 (WILAYA→ADMIN bootstrap + نافذة إهمال كلمة المرور) | ✅ مكتمل | 2026-08-05 | 16 اختبار تكامل `identity_bootstrap_tests` + `IdentityBootstrapState` + Root-pin + `get_identity_status`/`begin_wilaya_provision`/`finalize_wilaya_provision`/`issue_first_admin_key`/`begin_challenge`/`complete_challenge` + `metadata.auth_method` telemetry + بوابة كاملة (`cargo test` 633 lib + clippy `-D warnings`/`check:arch`/`check`/Vitest 86) |
| B6-A (Authentication Cutover) | ✅ مكتمل | 2026-08-06 | `IdentityAuthenticationPolicy` كمصدر قرار وحيد + `LoginResponse.identity_challenge_required` + إغلاق بوابة كلمة المرور عند وجود هوية ADMIN نشطة (ACTIVE cert + `.adminkey`) + `GRPC_LEGACY_AUTH` مؤقت + إزالة البذر الإنتاجي (`should_seed_legacy_admin`/`GRPC_LEGACY_BOOTSTRAP`) + إغلاق Rule 128 + الواجهة (تبويب `.adminkey` افتراضيًا عند AdminProvisioned/Ready) + بوابة كاملة (`cargo test` 633 lib + 30 تكامل + clippy `-D warnings`/`check:arch`/`check`/Vitest 88) |
| Commit ② (UNIT CSR bootstrap — B6-A isolation) | ✅ مكتمل | 2026-08-06 | `NodeIdentityResolver` (R5) + `IdentityTrustAnchorService` (مرساة ثقة WILAYA مستقلة) + مسار إصدار موحّد (`sign_identity_request`/`generate_identity_request`/`sign_unit_identity_request`/`finalize_unit_provision`) + سلسلة UNIT (`Uninitialized → UnitWaitingForCertificate → UnitActive`) + 4 أوامر IPC (`begin_unit_provision`/`sign_unit_identity_request`/`finalize_unit_provision`/`install_wilaya_certificate`) + 19 اختبار تكامل UNIT (16 WILAYA دون تغيير) + الواجهة (قسم UNIT على شاشة الدخول) + إعادة معايرة `contracts.snapshot.json` + بوابة كاملة (644 lib + كل التكامل + clippy `-D warnings`/`check:arch`/`check`/Vitest 88) |
| Commit ③ (Authentication Final Cutover — B6-B الجزء الأول) | ✅ مكتمل | 2026-08-06 | حذف `GRPC_LEGACY_AUTH` نهائيًا (من `IdentityAuthenticationPolicy` و`commands/auth.rs`؛ لم يبقَ متغير بيئة) + إغلاق `change_password` كأمر IPC وخدمة وواجهة + بوابة دائمة: مسار كلمة المرور يبقى فقط عند غياب هوية ADMIN نشطة (مستخدمي UNIT المحليون + العقد غير المجهَّزة) + إزالة `UserService::change_password`/`get_user_node_id` + إعادة معايرة `contracts.snapshot.json` + بوابة كاملة (644 lib + كل التكامل + clippy `-D warnings`/`check:arch`/`check`/Vitest 88) |
| Commit ④a (بنية B6-B التحتية — سلوكيًا محايد) | ✅ مكتمل | 2026-08-06 | سجل مُنتِج `sync_issuer_sequence_state` (مفتاح `identity_id`، `begin_export`/`PendingIssuedSequence::commit` — advance-on-success) + `Ed25519PackageSigner::from_provider`/`public_key_hex` + فحص تسلسل متسلسل + `import_export.rs` دون لمس + اختبارات (7 repo + 1 signer) + بوابة كاملة |
| Commit ④b (القطع السلوكي B6-B — `IdentitySignedExportService`) | ✅ مكتمل | 2026-08-06 | `IdentitySignedExportService::export_v2_package` (حلّل R5 fail-closed بلا تراجع HMAC + تخصيص/التزام التسلسل + توقيع Ed25519) + قلب أوامر الإنتاج الأربعة (products/daily_report/monthly_summary/stock_movements) إلى V2 بـ `issuer_identity_id`/`package_sequence`/`signature_version=2`/`signing_key_id` + `export_hash` مستقل (Uuid) عن `package_id` + `.unit` تبقى HMAC (استثناء Bootstrap) + 5 اختبارات تكامل `sync_v2_producer_export_tests` (metadata/round-trip Guard/replay/out-of-order/failed-export-reuses-sequence/rotation/unprovisioned fail-closed) + تحديث RFC/ADR-0038 + بوابة كاملة |
| Commit ⑤a (بنية B7 التحتية — سلوكيًا محايد) | ✅ مكتمل | 2026-08-06 | `IdentityRotationService` (مخطّط/محقِّق خالص: `plan` → CSR غير موقّع لـ Rotate/Re-Issue مع `IDENTITY_ALGORITHM_PROFILE_ED25519`؛ `verify_finalize` fail-closed: Replay صفر-كتابة، R5، ربط الموضوع، ACTIVE، Credential Guard) + مرحلة مفتاح عقدة `NodeKeyStore::write_pending`/`promote_pending`/`discard_pending`/`read_pending` (ملف `node_identity.key.pending`، age::x25519، ترقية idempotent) + 12 اختبارًا + بوابة كاملة (`cargo test` 678 lib + clippy `-D warnings`/`check:arch` صفر) |
| Commit ⑤b (القطع السلوكي B7 — credential rotation) | قيد التنفيذ | — | 5 أوامر IPC post-auth (`begin_wilaya_rotation`/`finalize_wilaya_rotation`/`begin_unit_rotation`/`sign_unit_rotation_request`/`finalize_unit_rotation`) + `IdentityRotationCoordinator` + Trust Package داخل `finalize_wilaya_rotation` (قبل ترقية السر، توقيع بالمفتاح القديم) + `Action::RotateCredential`/`ReissueCredential` + `AuditAction::IdentityRotated`/`IdentityReissued` + Issuer Local State لتسجيل UNIT + عقود الواجهة + إعادة معايرة `contracts.snapshot.json` + `identity_rotation_tests` + بوابة كاملة + `tauri build` |
| B8 ① (بنية مزامنة الحسابات — `identity_access`) | قيد التنفيذ | — | ADR-0040 (مسودة، Invariants 11/12/13) + فصل محفظة كلمات المرور (`hash_admin`/`verify_admin` نطاق-admin كأسطول + `hash_node`/`verify_node` مرتكزة على العقدة؛ `GLOBAL_ADMIN_DOMAIN` داخل المُزوِّد فقط) + حارس تسجيل دخول حسب الدور (Admin: `verify_admin` ثم تراجع `verify_node`؛ User: `verify_node`) + `deleted` (soft-delete) في مستودع المستخدمين + `upsert_synced_admin`/`upsert_synced_user` (keep-id) + `UserAccountSyncService` (set_fleet_admin_password/set_unit_user_password/set_account_status/export fail-closed/apply مع reconcile rename) + حمولة `identity_access` (hash admin مشترك + hash user خاص بالوحدة) + AuthZ (`ManageAccountSync`/`ExportIdentityAccessPackage`/`ImportIdentityAccessPackage`) + إغلاق Rule 127 (حارس ManageUnits عبر `ResourceContext::WilayaNode`) + `AuditAction` لـ B8 |

بشكل محدد، يُغلَق B3 وفق الحالة المعتمدة:

- **مسار موحّد لإنشاء الجلسات:** `SessionEstablishmentService` واحد لدخول كلمة المرور
  ودخول التحدي (تمرير `session_id` للتحدي) — بدون سطح IPC جديد.
- **قاعدة الحوكمة 126:** استخدام Ed25519 محصور في طبقات الهوية
  (`domain/identity`, `infrastructure/identity`, `application/services/identity_*`).
- **قاعدة الحوكمة 17:** بناء المستودعات عبر `RepositoryProvider`
  (`executor.identity_store()`) بدل الإنشاء اليدوي داخل الخدمات.
- **اختبارات أمنية تغطي:** Replay (Fail-Closed), Tampered Signature, Wrong Passphrase,
  Foreign Node، و Portability للـ `.adminkey` (كلمة مرور فقط دون أسرار عقدة).

بشكل محدد، يُغلَق B4 وفق الحالة المعتمدة:

- **توقيع V2 (`signature_version = 2`):** `Ed25519PackageSigner`/`Ed25519PackageVerifier`
  + `canonical_bytes_for_signature`؛ يبقى HMAC V1 للقراءة خلال نافذة الإهمال.
- **تحقق موحّد:** `SyncPackageIdentityVerificationService::verify_v2_signature` يُنفَّذ داخل
  `run_import_pipeline` قبل Transport Guard؛ فشل مغلق مقابل شهادة المُصدِر من Identity Store.
- **Transport Guard:** `(issuer_identity_id, package_sequence)` داخل `run_import_pipeline`
  فقط؛ Replay/Out-Of-Order → `OperationNotPermitted` ولا يُستهلك التسلسل؛
  `advance_issuer_sequence` في نفس المعاملة.
- **أمرا** `import_trust_package`/`import_registry_package` (kind `trust`/`registry`) عبر نفس
  المسار + authz WILAYA-admin (الوحدات ليست موزِّعة ثقة: Root → WILAYA → UNIT) +
  `AuditAction::ImportTrustPackage`/`ImportRegistryPackage`.
- **شهادات داخل الحزم إلزامية التوقيع** (ADR-0039 §5 مسار واحد)؛ سجلات Root تبقى
  `signature: None` (سجل سلطة غير قابل للتوزيع).

بيانات/كود موجود:
- جدول `users` وبياناته: تُحافظ عليه أثناء النافذة (additive فقط).
- حزم V1 الموقّعة HMAC: تبقى قابلة للقراءة.
- admin الافتراضي: يُستبدل عبر مسار bootstrap الجديد؛ لا حذف مدمر.

بشكل محدد، يُغلَق B5 وفق الحالة المعتمدة:

- **توقيع Root بالكامل دون اتصال:** Root لا يمسّ التطبيق إطلاقًا. العقدة تصدّر
  `generate_wilaya_request()` (CSR بشهادة ذاتية المفتاح WILAYA)، يوقّعها Root خارجيًا،
  ثم تستوردها العقدة عبر `finalize_wilaya_provision(signed_cert)`:
  التحقق من توقيع Root + مطابقة المفتاح العام لمفتاح العقدة + المُصدِر `None` (Root)
  + Upsert WILAYA ACTIVE. `provision_wilaya`/`issue_admin` يبقيان للاختبار فقط.
- **تثبيت المفتاح العام لـ Root:** `infrastructure/identity/root_public_key.rs` —
  `PROD_ROOT_PUBLIC_KEY` مُثبّت (RFC 8032 §7.1 TEST 2 placeholder) + `GRPC_ROOT_PUBLIC_KEY`
  override + fallback dev لـ `debug_assertions` (TEST 1؛ سرّه `TEST_ROOT_SECRET`).
  يُحلّ لاحقًا عبر `resolve_root_public_key()` — وليس في
  `validate_production_security_environment()`.
- **بوابة بذر admin القديم:** `ConnectionFactory::new()` يبذر admin الإرثي فقط عند وجود
  صف admin مسبقًا أو `GRPC_LEGACY_BOOTSTRAP=1` عبر `should_seed_legacy_admin`؛
  `create_default_admin` يبقى موسومًا `[arch:allow-bootstrap-admin]` (Rule 128) خلال
  نافذة الإهمال حتى B6.
- **حالة bootstrap صريحة:** `IdentityBootstrapState` (مشتقة، غير متسلسلة):
  `Uninitialized → WaitingForRootCertificate → WilayaActive → AdminProvisioned → Ready`
  تُحسب عبر `IdentityBootstrapStatusService::compute()` وتُقارن من بوابة `get_identity_status`.
- **ربط أول مسؤول:** `issue_first_admin_key` يربط صف `users` بهاش فارغ = هوية فقط؛
  المستخدمون ذوو الهاش السابق يحتفظون بدخول كلمة المرور (additive). `complete_with_passphrase`
  يفكّ تشفير `.adminkey` ويوقّع التحدي في Rust — الواجهة لا ترى المفاتيح أبدًا.
- **إغلاق الفشل:** إعادة استيراد مطابقة → صفر كتابة (idempotent)؛ أي اختلاف حقيقي →
  Fail-Closed. `is_identical_to` هو التعريف الوحيد لتكافؤ الشهادات (الحقائق + التوقيع،
  دون `package_sequence`/`status`/`not_after`). Replay/Wrong Passphrase → Fail-Closed.
- **توافق V1:** كلمة مرور الإرث صالحة عبر النافذة؛ تسجيل الدخول بالكلمة المسجَّل
  `metadata.auth_method = password` + `LEGACY_PASSWORD_LOGIN` warn. جدول `users` محفوظ.

بيانات/كود موجود:
- جدول `users` وبياناته: تُحافظ عليه أثناء النافذة (additive فقط) — الهاش يُصفَّر
  عند اعتماد هوية أول مسؤول فقط.
- admin الافتراضي: لا حذف مدمر؛ يُستبدل عبر مسار bootstrap الجديد.
- الجبهة: علامة تبويب `.adminkey` + خطوة WILAYA bootstrap على شاشة الدخول فقط.

بشكل محدد، يُغلَق B6-A وفق الحالة المعتمدة:

- **مصدر القرار الوحيد للمصادقة:** `IdentityAuthenticationPolicy` (طبقة
  application) — القرار مبني على الحقيقة الأمنية (ACTIVE ADMIN cert + `.adminkey`
  موجود) وليس على `IdentityBootstrapState` المشتقة (خاصة بالـ UI/مسار bootstrap).
  `has_active_admin_identity` تمنع حالة lockout: شهادة دون ملف مفتاح تُبقي مسار
  كلمة المرور مفتوحًا.
- **بوابة تسجيل الدخول:** `commands/auth.rs::login` يرفض مسار كلمة المرور عند
  وجود هوية ADMIN نشطة ويعيد `LoginResponse.identity_challenge_required = true`
  (additive عبر `#[serde(default)]`).
- **`GRPC_LEGACY_AUTH` (مؤقت):** `1` يعيد فتح مسار كلمة المرور. موثق في الكود
  والـ ADR: Introduced B6-A / Removed B6-B / MUST NOT survive after B6-B.
- **إزالة البذر الإنتاجي:** `ConnectionFactory::new()` لم يعد يبذر `admin/admin`؛
  حُذف `should_seed_legacy_admin` و`GRPC_LEGACY_BOOTSTRAP`. `db::seed_default_admin`
  (دعم اختبارات فقط) مستخدم حصريًا في `new_for_test`/`new_with_path` (DBs
  `Uninitialized` → البوابة تُبقي كلمة المرور مفتوحة → صفر كسر في الاختبارات).
- **إغلاق Rule 128:** مع غياب أي بذر إنتاجي، أُقفلت القاعدة بدلًا من كبتها.
- **الواجهة:** تبويب `.adminkey` افتراضي عند `AdminProvisioned`/`Ready` + إخفاء
  تبويب كلمة المرور + معالجة `identity_challenge_required` دفاعيًا.

بشكل محدد، يُغلَق Commit ② (UNIT CSR bootstrap) وفق الحالة المعتمدة:

- **مُحلِّل العقدة المحلي (R5):** `NodeIdentityResolver::resolve_local_signer` —
  المصدر الوحيد لحل الموقّع المحلي (مفتاح العقدة + شهادة ACTIVE من Identity Store)؛
  fail-closed على أي خلل (مفتاح غير موجود → `Ok(None)`؛ غير ACTIVE؛ خوارزمية خاطئة؛
  عدم تطابق `signer.public_key() == certificate.public_key` — R5).
- **مسار الإصدار الموحّد (D2):** `sign_identity_request` يملك ربط `issuer_identity_id`
  + التوقيع + بيانات الاعتماد لكل شهادة يصدرها node (ADMIN/UNIT ومستقبلًا Rotate/
  Re-Issue/Recovery). WILAYA (صادرة من Root) لا تمرّ بهذا المسار.
  `generate_identity_request` يولّد CSR لكل `subject_type` (يُكتب المفتاح فقط عبر
  `node_key_store`؛ حارس انتقال لمرة واحدة عند وجود مفتاح).
- **توقيع CSR UNIT على WILAYA:** `sign_unit_identity_request` يتحقق أن `subject_type`
  هو UNIT وأن `CSR.subject_id` يطابق `units.get_unit` (تحقق من المصدِّر، لا فرض) ثم
  يوقّع عبر `NodeIdentityResolver` (شهادة WILAYA ACTIVE + R5).
- **خطوتان صارمتان:** `install_wilaya_certificate` خدمة مستقلة
  (`IdentityTrustAnchorService`) — لا تقرأ `NodeKeyStore`، لا تفحص حالة UNIT، لا تمسّ
  `IdentityBootstrapState`؛ `AlreadyInstalled` حصريًا لنفس الشهادة حرفيًا (`is_identical_to`)،
  وأي `identity_id` مع `credential_id` مختلف → Fail-Closed.
- **إنهاء إمداد UNIT:** `finalize_unit_provision` يحلّ المُصدِر عبر
  `issuer_identity_id → get_by_identity_id` ثم يتحقق (موجود + ACTIVE + WILAYA)؛ يتحقق
  من التوقيع مقابل المفتاح العام للمُصدِر + مطابقة مفتاح العقدة + مطابقة `subject_id`
  لصف `units` محلي. idempotent بلا كتابة عند إعادة تقديم مطابقة.
- **سلسلة حالة UNIT:** `Uninitialized → UnitWaitingForCertificate → UnitActive`؛
  `IdentityBootstrapStatusService::compute()` تتفرّع على النص الخام `"UNIT"`
  (`get_node_type`)، مع افتراض WILAYA — بذرة `UNCONFIGURED` لا تُعدّ عقدة UNIT.
- **4 أوامر IPC جديدة (pre-auth):** `begin_unit_provision` / `sign_unit_identity_request`
  / `finalize_unit_provision` / `install_wilaya_certificate`؛ 19 اختبار تكامل
  `identity_unit_bootstrap_tests`؛ سلسلة WILAYA الـ 16 اختبارًا دون تغيير.

بشكل محدد، يُغلَق Commit ③ (Authentication Final Cutover — الجزء الأول من B6-B)
وفق الحالة المعتمدة:

- **حذف `GRPC_LEGACY_AUTH` نهائيًا:** أُزيل `legacy_auth_override()` من
  `IdentityAuthenticationPolicy` — لم يبقَ أي متغير بيئة يفتح مسار كلمة المرور
  (Introduced B6-A / Removed B6-B / MUST NOT survive after B6-B). `login` في
  `commands/auth.rs` لم يعد يقرأ البيئة.
- **بوابة دائمة:** `password_login_allowed = !has_active_admin_identity` —
  مسار كلمة المرور يُبقي مفتوحًا فقط عند غياب هوية ADMIN نشطة (حالة أمنية:
  ACTIVE ADMIN cert + `.adminkey`). هذا يحافظ على **Application User
  Authentication** لمستخدمي UNIT المحليين (منشأون من `UserExport { username,
  password_hash, role }` داخل `.unit` package) وللعقد غير المجهَّزة، بينما عقد
  WILAYA ذات هوية ADMIN نشطة تُوجَّه حصريًا إلى **Operator Authentication**
  (Challenge–Response / `.adminkey`). `IdentityAuthenticationPolicy` يبقى كما هو
  بمصدر القرار الوحيد.
- **إزالة `change_password`:** حُذف الأمر IPC `change_password` من
  `commands/auth.rs` و`commands/registry.rs`، وحُذفت خدمة `UserService::change_password`
  وأسلوب `get_user_node_id` من المستودع، وحُذفت دالة الواجهة `changePassword`
  من `src/lib/tauri.ts` و`user.contract.ts` وأُعيدت معايرة
  `contracts.snapshot.json`. يبقى `users` read-only خلال النافذة؛ `create_user`
  والكتابة عبر `unit_service` للمستخدمين المحليين دون تغيير.
- **الواجهة:** شاشة الدخول تحافظ على تبويب كلمة المرور لعقد UNIT وغير المجهَّزة
  (غياب هوية ADMIN نشطة) وتبويب `.adminkey` لعقد WILAYA المجهَّزة — دون تغيير عن
  Commit ②.
- **اختبارات:** أُعيدت كتابة اختبار البوابة (بلا override)، وأُعيدت كتابة اختبار
  `modified_password_persists_across_restart` لاستخدام المستودع مباشرة (بلا
  `UserService::change_password`). البوابة كاملة: 644 lib + كل التكامل (صفر فشل) +
  clippy `-D warnings` + `check:arch` (صفر تحذيرات) + `check` (0 أخطاء) +
  Vitest 88/88.

ملاحظة معمارية (قرار لاحق، خارج Commit ③): يوجد نموذجان متميزان للمصادقة —
**Operator Authentication** (`.adminkey` + Challenge–Response لـ WILAYA/Admin) و
**Application User Authentication** (اسم مستخدم/كلمة مرور لمستخدمي UNIT
المحليين). أي إزالة مستقبلية لكلمة المرور كليًا تتطلب مشروعًا مستقلًا ينقل
مستخدمي UNIT إلى نموذج هوية (بما يشمله من تصميم تحدّي خاص بالعقدة) — لا يحققه
B6-B الحالي.

بشكل محدد، يُغلَق Commit ④b (القطع السلوكي B6-B) وفق الحالة المعتمدة:

- **`IdentitySignedExportService`:** المدخل الوحيد لتصدير V2 —
  `export_v2_package(dataset, source_node_id, kind, target_path, node_type, crypto_port)`
  يُرجع **التسلسل المصدر فقط**. الحل: `NodeIdentityResolver::resolve_local_signer`
  (R5) مع **فشل مغلق بلا تراجع HMAC** — عقدة بلا مفتاح أو بلا شهادة ACTIVE أو R5
  mismatch تُخطئ `OperationNotPermitted` ولا تُنتج ملفًا.
- **الحزم الأربع قلبها:** `export_products_package`/`export_daily_report_package`/
  `export_monthly_summary_package`/`export_stock_movements_package` صارت توقّع
  `signature_version = 2` (Ed25519) وتحمل `issuer_identity_id` (المُصدِر =
  `identity_id`، استمرارية عبر الدوران — §3.4.4) و`package_sequence` (مؤمّنة من
  `sync_issuer_sequence_state`) و`signing_key_id` (المفتاح العام للعقدة، hex).
  `.unit` (`export_unit_node_package`) **دون تغيير حرفيًا** — HMAC، استثناء Bootstrap (§3.10).
- **`export_hash` مستقل:** معرّف تتبّع مالي يولّد كـ `Uuid::new_v4()` مستقل كليًا عن
  `package_id` (لا ربط/مساواة في أي كود — عمود UNIQUE + عرض مختصر في الجدول الزمني فقط).
- **التحقق من عدم التقدم:** التزام التسلسل (advance-on-success) يحدث فقط بعد نجاح
  `PackageBuilder::build_encrypted_stream_path`؛ الفشل يُسقط الرمز ولا يحرق رقمًا.
- **اختبارات التكامل (5):** `sync_v2_producer_export_tests` — V2 metadata +
  تحقق التوقيع مقابل شهادة المُصدِر، round-trip مستهلك (Accept ثم Replay ثم
  OutOfOrder ثم متتالية)، فشل-بلا-تقدّم + إعادة استخدام نفس الرقم، استمرارية عبر
  دوران الشهادة (new `credential_id`/`generation`، نفس `identity_id`)، وfail-closed
  للعقدة غير المجهَّزة.

---

## 6. Rollback Plan (خطة التراجع)

- **يظل مساران فعّالان طوال نافذة الإهمال:** `users` + كلمة المرور، وHMAC للحزم
  الإرثية — فلا نقطة فشل واحدة.
- **تعطيل قبول Ed25519:** عبر إعداد بيئة/تشغيل دون حذف أي شيء (الترحيل إضافي فقط)
  → العودة إلى HMAC-only فوري.
- **لا DDL مدمر** في أي خطوة — التراجع آمن لأن الجداول الجديدة تُترك دون تفعيل.
- `signature_version` يظل `Option<u16>` فلا يلزم تعديل عقد متسلسل عند التراجع.

---

## 7. Backward Compatibility (التوافق الرجعي)

- حزم V1 (HMAC) تُقرأ وتُتحقق كما اليوم.
- حزم `.unit` تبقى موقّعة HMAC (استثناء Bootstrap، §3.10) — مسار استيرادها دون
  تغيير (يتجاوز `run_import_pipeline`).
- حزم الإنتاج الجديدة (V2/Ed25519) تُستهلك عبر `verify_v2_signature` + Transport
  Guard داخل `run_import_pipeline` — بنمط add-only؛ لا تعديل على حزم الإصدارات السابقة.
- دخول كلمات مرور المستخدمين الحاليين يستمر خلال نافذة الإهمال.
- `CurrentSession` و`authorize_command` وواجهات frontend (`tauri.ts`/contracts) دون تغيير.
- تُقبل حزم الثقة الجديدة فقط عند استيفاء: ترتيب نقل سليم (per-issuer sequence)
  + رتابة generation لكل credential + سلسلة ثقة سليمة — Fail-Closed.
- المستندات التاريخية تُعلَّم كتاريخية عند الدمج (وفق D2).

---

## ملحق: مراجع موثقة

| المرجع | الموقع |
|--------|--------|
| RFC-to-ADR Process | `docs/architecture/ARCHITECTURE_FREEZE.md` §4 |
| Freeze Contracts | `docs/architecture/ARCHITECTURE_FREEZE.md` §2 |
| ADR-0003 (مُعدَّل بعد الاعتماد) | `docs/architecture/0003-sync-protocol-versioning-and-integrity.md` |
| ADR-0004 Breaking Changes | `docs/architecture/0004-protocol-changes-are-breaking-changes.md` |
| ADR-0006/0007/0008 (نافذة إهمال HMAC) | `docs/architecture/000{6,7,8}-*.md` |
| ADR-0019 / Rule 38 (age only) | `docs/architecture/0019-legacy-crypto-isolation.md` |
| الكود: users / auth | `src-tauri/src/db/migrations/001_initial.sql`, `src-tauri/src/commands/auth.rs` |
| الكود: التوقيع / الاستيراد | `src-tauri/src/infrastructure/sync/packages/`, `src-tauri/src/commands/import_export.rs` |
