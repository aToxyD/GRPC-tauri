# ADR 0038: بنية هوية العقد وسلسلة الثقة (Node Identity & Trust Architecture)

# الحالة
مقبول (Accepted)

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
- `algorithm_version` ثابت لكل credential صادر ولا يتغير إلا عبر Rotate/Re-Issue.

## 7. قواعد `check_arch` الجديدة (GROUP 24)
- **Rule 126**: منع Ed25519/هوية غير متماثلة خارج طبقات الهوية (commands/models/repositories).
- **Rule 127**: `Action::ManageUnits` يتطلب حارس node-type (WILAYA) عبر authz؛ مواقع غير
  منقّحة تُعلَّم `[arch:allow-manageunits-wilaya]`.
- **Rule 128**: منع اعتماد admin افتراضي مثبّت (`hash_password("admin"...`)؛ المواقع
  الحالية تُعلَّم `[arch:allow-bootstrap-admin]` حتى B5.
- **Rule 129**: `signature_version` يبقى `Option<u16>` (حماية نقطة التوسّع).

## 8. الـ Invariants (قوانين الهوية الثابتة)
identity_id ثابت؛ credential_id ثابت عبر التدوير؛ generation رتيب تمامًا؛ اعتماد ACTIVE
واحد لكل هوية؛ Root لا يوقّع ADMIN مباشرة؛ استقلال ترتيب الحزم عن إصدار الشهادات؛ لا
شهادة ذاتية التوقيع؛ لا ساعة حائط في التقييم؛ Identity Store مصدر الحقيقة الوحيد؛
algorithm_version ثابت لكل اعتماد.

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
- B6-B: قلب الافتراضي إلى `signature_version = 2`؛ HMAC للقراءة فقط؛ إزالة `GRPC_LEGACY_AUTH`/`change_password`.

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
