# ADR 0038: بنية هوية العقد وسلسلة الثقة (Node Identity & Trust Architecture)

# الحالة
مقبول (Accepted)

> **تم الاستبدال جزئيًا (2026-08-19 — ADR-0050):** قرار B6-B الخاص بالمصادقة
> العادية لمسؤول WILAYA (Commit ③ — البوابة الدائمة: `password_login_allowed =
> !has_active_admin_identity`، إلزام Challenge–Response لتسجيل الدخول العادي على
> عقد WILAYA المجهَّزة ذات هوية ADMIN نشطة) **مستبدل بقرار ADR-0050** فيما يخص
> المصادقة العادية فقط: تسجيل الدخول العادي = username + password، و`.adminkey`
> يبقى آلية الاسترداد/عالية الضمان. كل ما عدا ذلك (هوية العقد، سلسلة الثقة،
> الحارسان، دوران الشهادات، R5، Recovery Mode) **غير متأثر**. السجل التاريخي
> أدناه يُحفظ كما هو دون تعديل.

# التاريخ
2026-08-04

# المالك
Architecture / Security

# مرجع
RFC `docs/architecture/rfcs/2026-08-04-node-identity-trust.md` (معتمد بتاريخ 2026-08-04 —
Architecture Review Result: ACCEPT, No blocking architectural findings).
يُعدّل هذا القرار بند "التواقيع غير المتماثلة" في ADR-0003 (السماح بـ Ed25519).

# السياق
التحليل في المراحل C/D/E أثبت أن المشكلة الجوهرية هي Distributed Mutable State الخاصة
بالهوية والثقة، وليست المصادقة وحدها. الوضع الحالي: توقيع HMAC بمفتاح بيئة مشترك،
admin افتراضي `admin/admin` مثبّت في الكود، WILAYA بلا UUID، مرجع الوحدة غير متجانس،
و`signature_version` دائمًا `None`. الهدف: هوية عقدة غير متماثلة قابلة للتحقق، ونموذج
حياة مفاتيح صريح، وفصل تام بين ترتيب النقل وإصدار الشهادات وسلسلة الثقة.

# القرار

## 1. النموذج (Identity Model)
- كل كيان (WILAYA/UNIT/ADMIN) هو **هوية (Identity)** بمعرف ثابت `identity_id: UUID`
  و`subject_id: UUID` مرجعي؛ الأسماء والرموز Metadata فقط.
- `IdentityCertificate { identity_id, subject_type, subject_id, issuer_identity_id,
  credential_id, generation, status, public_key (Ed25519), not_after (إرشادي),
  algorithm_version, signature }`.
- **Identity Store** هو المصدر الوحيد لحقيقة الهوية؛ ADMIN/UNIT/WILAYA كيانات من
  نفس الفئة، منفصلة عن جدول `users` (مسار إرث خلال النافذة).
- `IdentityState { credential_id, generation, issuer_identity_id, status, package_sequence }`
  (حقل `package_sequence` أثر تدقيق؛ سجل الترتيب الفعلي للنقل منفصل في `run_import_pipeline`).

## 2. دورة حياة الاعتماد
- Rotate = نفس `credential_id` + `generation++` + مفتاح جديد.
- Re-Issue = `credential_id` جديد + `generation = 1`.
- حالات صريحة: `ACTIVE → REVOKED | SUPERSEDED | EXPIRED`؛ `EXPIRED` تصدرها السلطة
  ولا تُشتق من ساعة حائط؛ `not_after` إرشادي فقط.

## 3. الحارسان المستقلان (فصل الطبقات)
- **Transport Guard**: `(issuer_identity_id, package_sequence)` — داخل `run_import_pipeline`
  فقط؛ `incoming.sequence == last_applied_sequence[issuer] + 1`.
- **Credential Guard**: `(credential_id, generation)` — `incoming.generation >
  stored_generation`؛ رفض الرجوع لإصدار أقدم بلا اعتماد على ترتيب الحزم.
- Transport Guard لا يعتمد على generation أبدًا، وCredential Guard لا يعتمد على ترتيب
  الحزم أبدًا.

## 4. سلسلة الثقة
- `Authority Root (offline, escrow) → WILAYA → UNIT/ADMIN`.
- Root يوقّع **WILAYA فقط**؛ لا UNIT، لا ADMIN، لا حزم؛ Root ليس CA تشغيلية.
- Bootstrap: تثبيت هوية WILAYA (موقّعة من Root، تُزوَّد فيزيائيًا) ثم إصدار ADMIN الأولى
  منها؛ **لا شهادة ADMIN ذاتية التوقيع إطلاقًا**.
- **Recovery Mode** خارج نموذج الثقة (استرداد تشغيلي وليس جزءًا من سلسلة الشهادات).

## 5. المصادقة
- Challenge–Response: رسالة ثابتة البنية `{ protocol_version, session_id,
  node_identity_id, nonce, challenge_version }`، توقيع Ed25519 على التمثيل القانوني،
  تحقق `ct_eq`؛ بلا timestamps.
- ملف `.adminkey` محمول مكتفٍ ذاتيًا (البنية النهائية في ADR-0039): مفتاح خاص مشفّر
  بـ `age::scrypt` + شهادة موقّعة (تتضمن credential_id وgeneration) + format_version +
  algorithm_version.

## 6. الحزم والهجرة
- حزم `trust` (شهادات + إبطالات) و`registry` (حالة الأسطول) عبر `run_import_pipeline`.
- `signature_version = 2` = Ed25519؛ يبقى HMAC لقراءة V1 خلال نافذة إهمال بنمط ADR-0007.
  منذ Commit ④b: حزم الإنتاج الأربع (products/daily_report/monthly_summary/
  stock_movements) تُصدَّر حصريًا V2/Ed25519 عبر `IdentitySignedExportService` بلا مسار
  تراجع HMAC (عقدة غير مجهَّزة → فشل مغلق).
  - **تعديل مرآة (2026-08-14 — RFC-AMENDMENT GOVERNANCE GATE، مرجع RFC §3.10):**
    استُبدل «استثناء Bootstrap الدائم» لـ `.unit` بـ **Trust-First V2/Ed25519 معياري**
    (اعتماد ADR-0044 A44-01/07/08/09/10/12 وADR-0045 A45-08): `.unit` يُوقَّع V2/Ed25519
    بهوية WILAYA ويُتحقق عبر مرساة WILAYA ACTIVE مثبَّتة **قبل قبوله** (موثوقة بسلسلة
    الجذر)؛ دور `.unit` = **User فقط** (`role=Admin` غير صالح)؛ أول حزمة V2 بـ
    `package_sequence = 1`؛ لا سر توقيع أسطوري (HMAC) على UNIT ولا مفتاح WILAYA خاص؛
    لا استبدال صامت للمرساة؛ فشل مغلق عبر WILAYA؛ التراجع = إعادة توفير؛ دوران الجذر
    بنافذة قبول صريحة. HMAC-V1 (.unit) أصبح **إرثًا قيد التقاعد** — قراءة إرثية فقط
    خلال نافذة إغلاق مبنية على الأدلة (A44-07) بانتقال fleet-sync (A44-09)؛ لا إصدار
    حزم V1 جديدة. السياق التاريخي: استُخدم HMAC لأن مسار `.unit` لم يكن يمر على
    `run_import_pipeline` ولم تكن تملك UNIT مرساة/هوية — حُلَّ بتركيب المرساة قبل
    القبول؛ لا تُصدَّر حزم HMAC عبر `IdentitySignedExportService`. السجل الكامل:
    ADR-0044 §28.6 / ADR-0045 §26.6.
- `algorithm_version` ثابت لكل credential صادر ولا يتغير إلا عبر Rotate/Re-Issue.

## 7. قواعد `check_arch` الجديدة (GROUP 24)
- **Rule 126**: منع Ed25519/هوية غير متماثلة خارج طبقات الهوية (commands/models/repositories).
- **Rule 127** (أُغلقت في B8 ①): `Action::ManageUnits` يتطلب حارس node-type (WILAYA) عبر
  authz؛ أُغلقت القاعدة — حارس `ResourceContext::WilayaNode` في `policies/mod.rs` وليس
  علامة `[arch:allow-manageunits-wilaya]`.
- **Rule 128**: منع اعتماد admin افتراضي مثبّت (`hash_password("admin"...`)؛ المواقع
  الحالية تُعلَّم `[arch:allow-bootstrap-admin]` حتى B5.
- **Rule 129**: `signature_version` يبقى `Option<u16>` (حماية نقطة التوسّع).

## 8. الـ Invariants (قوانين الهوية الثابتة)
identity_id ثابت؛ credential_id ثابت عبر التدوير؛ generation رتيب تمامًا؛ اعتماد ACTIVE
واحد لكل هوية؛ Root لا يوقّع ADMIN مباشرة؛ استقلال ترتيب الحزم عن إصدار الشهادات؛ لا
شهادة ذاتية التوقيع؛ لا ساعة حائط في التقييم؛ Identity Store مصدر الحقيقة الوحيد؛
algorithm_version ثابت لكل اعتماد.

## 9. دوران الاعتماد (B7 — Credential Rotation)
- **مسرحان:** `begin_*_rotation` يمرّح مفتاح عقدة جديدًا فقط (`node_identity.key.pending`،
  age::x25519) ويصدّر CSR غير موقّع؛ المفتاح النشط يبقى موثوقًا حتى `finalize` يرتّقي
  المرحّل. فشل التخطيط يرفض المرحّل (لا بقايا).
- **Rotate** = نفس `credential_id` + `generation+1` + مفتاح جديد؛ **Re-Issue** =
  `credential_id` جديد + `generation=1` (هوية ثابتة في الحالتين).
- **Finalize fail-closed:** توقيع (Root لـ WILAYA؛ المُصدِر ACTIVE+WILAYA لـ UNIT) + R5
  (مطابقة مفتاح العقدة للمرحّل) + ربط الموضوع + ACTIVE + Credential Guard (رفض
  Rollback/RejectZero). Replay المطابق → صفر كتابة (idempotent).
- **حزمة ثقة الدوران:** `finalize_wilaya_rotation` يكتب Trust Package (kind `trust`) —
  موقَّعًا **بالمفتاح القديم** (ما زال ACTIVE وقت الكتابة، قبل ترقية السر) — إلى مسار
  ملف يوفّره المشغّل؛ ثم يرتّقي المفتاح. أي فشل لاحق يستعيد المفتاح القديم (لا حالة
  R5 مكسورة). لا أمر IPC لتصدير حزم (تدفّق ملفات فقط).
- **توزيع WILAYA:** حصريًا عبر حزمة الثقة؛ **تسجيل UNIT في سجل WILAYA أثناء**
  `sign_unit_rotation_request` هو **Issuer Local State** (سجل تتبّع إصدار، افتراضيًا
  SUSPENDED محل ACTIVE السابق) وليس قناة توزيع — حارس الاعتماد على جانب WILAYA
  best-effort (نزاع → تحذير + تخطٍّ دون تعطيل التوقيع)، والقرار النهائي عند
  `finalize_unit_rotation` على UNIT (fail-closed). تفاصيل §3.12 من RFC.
- **Authorization/Audit:** `Action::RotateCredential`/`ReissueCredential` (نفس سياسة
  `authorize_authenticated`) + `AuditAction::IdentityRotated`/`IdentityReissued`.

# النتائج المترتبة
- توقيع قابل للتحقق (Ed25519) مرتبط بهوية العقدة بدل مفتاح env مشترك.
- نموذج حياة مفاتيح صريح وموثّق في سجل التدقيق.
- فصل طبقات يمنع اختلاط Transport Ordering بـ Credential Lifecycle بالـ Trust Chain.
- هجرة تدريجية دون Big Bang؛ HMAC وusers يبقيان خلال النافذة.

# خارج النطاق
- PKI/X.509 والشهادات الكاملة (تبقى خارج النطاق بموجب ADR-0003 المعدّل).
- تغيير نموذج التفويض (Authorization) أو الجلسة (CurrentSession) أو منطق الأعمال.
- التوقيع على المحتوى المالي/المحاسبي داخل الحزم (لا يتغير).

# خطة التنفيذ
- B1 (هذا القرار + تعديل ADR-0003 + تحديث Freeze §2.7 + قواعد check_arch + AGENTS.md).
- B2: مخطط Identity Store + UUID لـ WILAYA + توحيد مرجع الوحدة.
- B3: مفاتيح Ed25519 + Challenge–Response + `.adminkey`.
- B4: حزم trust/registry + الحارسان في `run_import_pipeline`.
- B5: استبدال bootstrap admin + نافذة إهمال users/كلمات المرور.
- B6-A: إغلاق بوابة كلمة المرور عند وجود هوية ADMIN نشطة (Authentication Cutover).
- B6-B: (Commit ③ — إزالة `GRPC_LEGACY_AUTH`/`change_password` وبوابة دائمة) ثم
  (Commit ④ — قلب الافتراضي إلى `signature_version = 2`؛ HMAC للقراءة فقط؛ يُسلَّم
  على مرحلتين: ④a بنية تحتية سلوكيًا محايدة، ثم ④b القطع السلوكي عبر
  `IdentitySignedExportService`).

# حالة التنفيذ
- **B1–B5 مكتملة** (B5 بتاريخ 2026-08-05). البوابة كاملة: `cargo test` (صفر فشل —
  633 lib/1134 كليًا)، `cargo clippy --all-targets -- -D warnings`، `bun run check:arch`
  (صفر تحذيرات)، `bun run check` (0 أخطاء)، Vitest 86/86. سجل التفاصيل في RFC §5
  (Progress Log) — كل من B1–B5.
- أوامر `import_trust_package`/`import_registry_package` مفعّلة عبر `run_import_pipeline`
  مع authz WILAYA-admin + `AuditAction::ImportTrustPackage`/`ImportRegistryPackage`.
- B5: `get_identity_status`/`begin_wilaya_provision`/`finalize_wilaya_provision`/
  `issue_first_admin_key`/`begin_challenge`/`complete_challenge` مفعّلة (bootstrap
  WILAYA→ADMIN بتوقيع Root دون اتصال + `IdentityBootstrapState` + `metadata.auth_method`
  telemetry)؛ كلمة مرور الإرث تبقى صالحة خلال النافذة (additive) حتى B6.
- **B6-A (Authentication Cutover) مكتملة** (2026-08-06): `IdentityAuthenticationPolicy`
  هو مصدر القرار الوحيد — مسار كلمة المرور يُقفل عند وجود هوية ADMIN نشطة (ACTIVE
  cert + `.adminkey`)؛ `LoginResponse.identity_challenge_required`؛ `GRPC_LEGACY_AUTH=1`
  override مؤقت (Introduced B6-A / Removed B6-B / MUST NOT survive after B6-B)؛
  إزالة البذر الإنتاجي (`should_seed_legacy_admin`/`GRPC_LEGACY_BOOTSTRAP`) وإغلاق
  Rule 128؛ الواجهة تُخفي تبويب كلمة المرور وتجعل `.adminkey` افتراضيًا عند
  AdminProvisioned/Ready. البوابة كاملة (633 lib + 30 ملف تكامل، clippy `-D warnings`,
  `check:arch` صفر تحذيرات، `check` 0 أخطاء، Vitest 88/88).
- **Commit ② (UNIT CSR bootstrap — B6-A isolation) مكتمل** (2026-08-06): `NodeIdentityResolver`
  (المصدر الوحيد لحل الموقّع المحلي، R5) + `IdentityTrustAnchorService` (تثبيت شهادة
  WILAYA كمرساة ثقة محلية — خدمة مستقلة: لا تقرأ `NodeKeyStore`، لا تفحص حالة UNIT، لا
  تمسّ `IdentityBootstrapState`؛ قابلة لإعادة الاستخدام في B7 rotation) + مسار إصدار
  موحّد (`sign_identity_request`/`generate_identity_request`/`sign_unit_identity_request`/
  `finalize_unit_provision` — المُصدِر يُحلّ عبر `issuer_identity_id → get_by_identity_id`
  ثم يتحقق ACTIVE + WILAYA) + `subject_id` محلي لعقدة UNIT
  (`settings.unit_code → units.get_unit_by_code → units.id`) + سلسلة حالة UNIT
  (`Uninitialized → UnitWaitingForCertificate → UnitActive`، افتراض WILAYA عند
  `UNCONFIGURED`) + 4 أوامر IPC (`begin_unit_provision`/`sign_unit_identity_request`/
  `finalize_unit_provision`/`install_wilaya_certificate`) + 19 اختبار تكامل
  `identity_unit_bootstrap_tests` (سلسلة WILAYA الـ 16 دون تغيير) + قسم UNIT على شاشة
  الدخول + إعادة معايرة `contracts.snapshot.json`. البوابة كاملة (644 lib + كل التكامل،
  clippy `-D warnings`، `check:arch` صفر تحذيرات، `check` 0 أخطاء، Vitest 88/88).
- **Commit ③ (Authentication Final Cutover — الجزء الأول من B6-B) مكتمل** (2026-08-06):
  حذف `GRPC_LEGACY_AUTH` نهائيًا (`IdentityAuthenticationPolicy` و`commands/auth.rs` بلا
  متغير بيئة) + إغلاق `change_password` كأمر IPC وخدمة وواجهة
  (`UserService::change_password`/`get_user_node_id` حُذفا) + **بوابة دائمة**:
  `password_login_allowed = !has_active_admin_identity` — مسار كلمة المرور يبقى فقط عند
  غياب هوية ADMIN نشطة (مستخدمي UNIT المحليون من `.unit` package والعقد غير المجهَّزة)،
  بينما عقد WILAYA ذات هوية ADMIN نشطة تُوجَّه حصريًا إلى Challenge–Response.
  `IdentityAuthenticationPolicy` يبقى مصدر القرار الوحيد. إعادة معايرة
  `contracts.snapshot.json` + اختبارات البوابة أُعيدت كتابتها (بلا override). البوابة
  كاملة (644 lib + كل التكامل، clippy `-D warnings`، `check:arch` صفر تحذيرات،
  `check` 0 أخطاء، Vitest 88/88).
- **Commit ④a (بنية B6-B — سلوكيًا محايد) مكتمل** (2026-08-06): سجل مُنتِج
  `sync_issuer_sequence_state` (مفتاح `identity_id` — استمرارية عبر الدوران،
  `begin_export`/`PendingIssuedSequence::commit` بفصل تخصيص/كتابة، advance-on-success)
  + `Ed25519PackageSigner::from_provider`/`public_key_hex` + فحص تسلسل متسلسل
  (consistency check) + `import_export.rs` دون لمس (إخراج مطابق بايتًا-ببايت).
- **Commit ④b (القطع السلوكي B6-B) مكتمل** (2026-08-06): `IdentitySignedExportService`
  — المدخل الوحيد لتصدير V2: `export_v2_package` يحلّ `NodeIdentityResolver` (R5)
  بفشل مغلق بلا تراجع HMAC، يخصّص تسلسلًا من `sync_issuer_sequence_state`، يوقّع
  Ed25519 عبر `PackageBuilder` (Metadata V2: `issuer_identity_id`/`package_sequence`/
  `signature_version=2`/`signing_key_id`)، ويُقدّم السجل فقط بعد نجاح كتابة الملف
  (الفشل لا يحرق رقمًا). أوامر الإنتاج الأربعة (`export_products_package`/
  `export_daily_report_package`/`export_monthly_summary_package`/
  `export_stock_movements_package`) قُلبت إلى V2؛ `export_hash` (Uuid) مستقل عن
  `package_id`؛ `.unit` تبقى HMAC دون تغيير (استثناء Bootstrap). 5 اختبارات تكامل
  `sync_v2_producer_export_tests` (metadata + تحقق التوقيع، round-trip مستهلك مع
  Accept/Replay/OutOfOrder، فشل-بلا-تقدّم + إعادة استخدام الرقم، استمرارية عبر دوران
  الشهادة، fail-closed للعقدة غير المجهَّزة). تحديث RFC §3.4.1/§3.10/§5/§7. البوابة
  كاملة (644 lib + كل التكامل + 5 اختبارات تكامل جديدة، clippy `-D warnings`،
  `check:arch` صفر تحذيرات، `check` 0 أخطاء، Vitest 88/88). **اكتمل بذلك الجزء
  الثاني من B6-B (قلب الافتراضي إلى `signature_version = 2`؛ HMAC للقراءة فقط).**
- **Commit ⑤a (بنية B7 التحتية — سلوكيًا محايد) مكتمل** (2026-08-06): `IdentityRotationService`
  (مخطّط/محقِّق خالص — `plan` يصدّر CSR Rotate/Re-Issue مع
  `IDENTITY_ALGORITHM_PROFILE_ED25519`؛ `verify_finalize` fail-closed: Replay صفر-كتابة،
  R5، ربط الموضوع، ACTIVE، Credential Guard) + مرحلة مفتاح العقدة
  (`NodeKeyStore::write_pending`/`promote_pending`/`discard_pending`/`read_pending` —
  `node_identity.key.pending`، age::x25519، ترقية idempotent). البوابة كاملة (`cargo test`
  678 lib + clippy `-D warnings` + `check:arch` صفر تحذيرات).
- **Commit ⑤b (القطع السلوكي B7)**: `IdentityRotationCoordinator` (تنسيق
  begin/finalize + حزمة ثقة الدوران داخل finalize + Issuer Local State لـ UNIT +
  تثبيت/استبدال ذرّي مع استعادة المفتاح القديم عند الفشل) + 5 أوامر IPC post-auth
  (`begin_wilaya_rotation`/`finalize_wilaya_rotation`/`begin_unit_rotation`/
  `sign_unit_rotation_request`/`finalize_unit_rotation`) + `Action::RotateCredential`/
  `ReissueCredential` + `AuditAction::IdentityRotated`/`IdentityReissued` + عقود الواجهة
  + `identity_rotation_tests` + إعادة معايرة `contracts.snapshot.json` + بوابة كاملة
  + `tauri build`.
