# ADR 0044: `.unit` Bootstrap Migration — Trust-First V2 (Ed25519) / HMAC-V1 Retirement

# الحالة (Status)

**Accepted — 2026-08-14 (Freeze §4 Step 3 — Final Freeze/ADR Status Governance Gate)**

> **السجل التاريخي (يُحفظ):** الوثيقة بدأت **Proposed — Decision for Owner Approval** (2026-08-14)، ثم اكتملت موافقة المالك على جميع القرارات (OWNER RATIFIED — بوابة اكتمال الاعتماد، سجل [ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md))، ثم نُفِّذ تعديل RFC §3.10/§3.12 ومرآة ADR-0038 §6 وثائقيًا (RFC-AMENDMENT GOVERNANCE GATE، §28.7)، وأُغلق **Freeze §4 Step 3** الآن: حالة **Accepted** + تحديث وثيقة التجميد (§2.2/§2.7/§2.8).

الاعتماد **معماري فقط**: التنفيذ **NOT AUTHORIZED** (يتطلب بوابة تفويض تنفيذ مستقلة)، والجاهزية الإنتاجية **محجوبة** بالعائق الخارجي A44-06 (اعتماد مفتاح Root الإنتاجي — ADR-0044 §9.4). لم يُغيّر أي كود أو اختبار أو إعدادات.

> **سجل تكميلي (SEC-007/SEC-008):** بند "تقاعد عائلة `GRPC_PACKAGE_SIGNING_KEY`/HMAC"
> (§8/§الجدول 407-414) اكتمل تنفيذه فعلياً بواسطة ADR-0047 (حزم المزامنة V2 حصراً)
> وADR-0048 (غلاف الإغلاق المالي `.fiscal-close.sync` → Ed25519 لهوية WILAYA؛
> إزالة `resolve_package_signing_key_32(_impl)` / `resolve_active_signing_key_id`
> / متغيرات التوقيع البيئية / صفر-مفتاح fallback). لا يُعدَّل النص التاريخي أعلاه.

# التاريخ (Date)

2026-08-14

# يعدّل / يستبدل (Supersedes / Amends)

| الوثيقة | البند | نوع التغيير |
|---------|-------|-------------|
| RFC `2026-08-04-node-identity-trust.md` | §3.10 — استثناء `.unit` الدائم (HMAC) | **Amends** (مقترح: إلغاء الاستثناء واستبداله بمسار V2 Trust-First) |
| RFC `2026-08-04-node-identity-trust.md` | §3.12 — قرارات مقرّرة (B6-A): ترتيب تسليم شهادة WILAYA (D2) | **Amends** (مقترح: تركيب مرساة الثقة قبل استيراد `.unit`) |
| RFC `2026-08-04-node-identity-trust.md` | §3.12 — بند 2 (طول نافذة إهمال HMAC) | **Clarifies** (نافذة مغلقة القرار — يبقى مفتوحًا هنا) |
| ADR-0038 | §3.10 mirror (بند "استثناء Bootstrap الدائم") | **Amends** (تبعية لنفس القرار) |
| ADR-0008 | سياسة الموقّعين الموثوقين (متغيرات البيئة) | **Supersedes** (مقترح: إحالة إلى التقاعد مع نافذة V1) |
| ARCHITECTURE_FREEZE.md | §2.7 "Sign: Ed25519 node identity" | **No conflict** — التجميد يصيغ HMAC كقراءة إرثية فقط (سطر 159)؛ لا يجمد `.unit`-HMAC. النقل إلى V2 **يتماشى** مع التجميد ولا يخالفه. |

لا تُعدَّل النصوص التاريخية لهذه الوثائق في هذه الخطوة؛ التعديل يتم بعد اعتماد هذا القرار، مع تسجيل كل تغيير.

> **نُفِّذ توثيقيًا (2026-08-14 — RFC-AMENDMENT GOVERNANCE GATE):** تعديل RFC
> `2026-08-04-node-identity-trust.md` (§3.10 + §3.12 D2 + صف B8 bootstrap في جدول
> B6-A) ومرآة ADR-0038 (§6) نُفِّذا وثائقيًا وفق اعتماد المالك. خطوة Step 3 النهائية
> من Freeze §4 (حالة ADR = Accepted + تحديث وثيقة التجميد) تبقى بوابة وثائقية لاحقة
> لأن تعديل وثيقة التجميد محظور في هذه البوابة. إحالة ADR-0008 **مؤجلة** حتى إغلاق
> نافذة V1 بالأدلة (§28.7).

# تصنيف أقسام الوثيقة (Document Category Taxonomy)

تُعلَّم الأقسام أدناه صراحةً حسب طبيعتها:

| الوسم | المعنى |
|-------|--------|
| **NORMATIVE** | قاعدة ملزمة تفرضها هذه الوثيقة بعد الاعتماد — أي تنفيذ مخالف هو انتهاك |
| **IMPLEMENTATION** | مطلب تنفيذي مستقبلي (عقد عمل لمرحلة لاحقة) — لا يُنفَّذ هنا |
| **OPERATIONAL** | إجراء تشغيلي/مراسم للمشغل — يُوثَّق في runbook لاحقًا |
| **OPEN DECISION** | قرار بيد المالك — لا يُحسم في هذه الوثيقة |
| **BLOCKING PREREQUISITE** | متطلب حاجز يجب حسمه قبل جاهزية الإنتاج — غير محلول هنا |

# خريطة حالة القرار (Decision Status Map)

الوضعيات الدقيقة المعتمدة في هذه الوثيقة: `CONFIRMED` / `PROPOSED` / `OWNER DECISION REQUIRED` / `BLOCKED` / `NOT IMPLEMENTED` / `NOT AUTHORIZED`.

## DECIDED (CONFIRMED)

| البند | الحالة |
|-------|--------|
| Trust-First V2 هو البنية الهدف لـ`.unit` | CONFIRMED (قرار معماري مقترح للمالك — الأدلة كاملة) |
| استثناء `.unit` HMAC (RFC §3.10) مُعيَّن للإزالة | CONFIRMED (نية القرار) — لم يُعدَّل النص بعد |
| Ed25519 يحل محل HMAC لأصالة `.unit` | CONFIRMED |
| شهادة WILAYA تُثبَّت قبل `.unit` | CONFIRMED (ممكنة بموجودات اليوم) |
| تسلسل الثقة Root → WILAYA → الحزمة | CONFIRMED |
| تشفير الحزم يبقى age/App Key دون تغيير | CONFIRMED |
| UNIT لا تملك سر توقيع أسطويًا | CONFIRMED (مبدأ ملزم) |
| التراجع = إعادة تهيئة كاملة، وليس ترقية تراجعية صامتة | CONFIRMED |

## BLOCKED

| البند | الحالة |
|-------|--------|
| اعتماد مفتاح Root الإنتاجي | BLOCKED — BLOCKING PREREQUISITE (§9.4) |

> ملاحظة تسوية (2026-08-14): بند «بنية أول استيراد B8» نُقل من جدول BLOCKED إلى قرار مالكي — **البنية (خيار A) أصبحت OWNER RATIFIED بشروط** في بوابة اعتماد المالك (سجل §26 في ADR-0045)، ثم **اكتمل اعتماد جميع قرارات المالك** في بوابة اكتمال الاعتماد (سجل §26.6 في ADR-0045) شاملةً A44-01؛ التنفيذ ما زال غير مصرح به. راجع §12.1 و§28.3 و§28.4 و§28.6 و[ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md).

## PENDING OWNER DECISION (OWNER DECISION REQUIRED)

> **اكتمل الاعتماد (2026-08-14) — لم يعد هناك أي قرار مالكي معلق**: جميع البنود أعلاه أقرّها المالك صراحةً في بوابة اكتمال الاعتماد (السجل الكامل: [ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md)). المعلق الوحيد: التنفيذ/تعديل RFC/خطوات الحوكمة غير المصرح بها — وليست قرارات مالكية.

**جميع القرارات المالكية اكتملت (Owner Ratification Completed — 2026-08-14)**: A44-01 (البنية) وA44-07 (معايير إغلاق نافذة V1 المبنية على الأدلة) وA44-08 (أول `package_sequence` = 1) وA44-09 (استراتيجية fleet-sync) وA44-10 (نافذة قبول صريحة للتدوير دون استبدال صامت) وA44-11 (نموذج الربط الحالي) وA44-12 (التراجع = إعادة توفير) — كلها **OWNER RATIFIED** (مصدر: تعليمات بوابة اكتمال الاعتماد، وسجلها الكامل في [ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md)). لا توجد بنود معلقة سوى التنفيذ والتعديلات الحوكمية غير المصرح بها (§28.6).

## NOT AUTHORIZED (في هذه البوابة)

| البند | الحالة |
|-------|--------|
| أي تنفيذ مصدري (Rust/TS/Svelte) | NOT AUTHORIZED |
| إزالة HMAC أو `GRPC_PACKAGE_SIGNING_KEY` | NOT AUTHORIZED |
| تغيير تحقق الإقلاع (startup validation) | NOT AUTHORIZED |
| تنفيذ تنسيق حزمة V2 لـ`.unit` | NOT AUTHORIZED |
| تنفيذ أي حل B8 أو استثناء Admin في وضع الإعداد | NOT AUTHORIZED |
| استبدال/توليد مفتاح Root | NOT AUTHORIZED |
| تعديل معالجة App Key أو ADR-0039/0041 | NOT AUTHORIZED |
| تعديل نص RFC أو إحالة ADR-0008 | NOT AUTHORIZED (حتى اعتماد المالك — §6.1) |

---

# 1. العنوان (Title)

نقل إقلاع `.unit` من HMAC-V1 (مفتاح سري مشترك أسطوري `GRPC_PACKAGE_SIGNING_KEY`) إلى التوقيع غير المتماثل Ed25519 (V2) المربوط بهوية WILAYA المرساة في Root، عبر ترتيب **Trust-First**: تركيب مرساة ثقة WILAYA على عقدة UNIT جديدة قبل استيراد `.unit` — ثم تقاعد كامل لعائلة إعدادات HMAC.

# 2. السياق (Context)

- RFC §3.10 (`2026-08-04-node-identity-trust.md:452-456`) يصنّف `.unit` كاستثناء دائم موقّع HMAC، مبررًا بالدائرة: «مسار استيراد `.unit` يتجاوز `run_import_pipeline` ولا تملك عقدة UNIT مرساة ثقة/هوية عند التزويد — تحتاج ثقة ← تحتاج `.unit`».
- RFC §3.11 (`:470-476`) يتبنى أصلاً نموذج **"Bootstrap بهوية WILAYA أولًا ثم إصدار ADMIN (المعتمد)"** — مرساة واحدة منذ أول لحظة.
- RFC §3.12 (D2, `:486-489`) يعرّف `install_wilaya_certificate()` كخدمة **مستقلة** (لا تقرأ `NodeKeyStore`، لا تفحص حالة UNIT، لا تمسّ `IdentityBootstrapState`؛ قابلة لإعادة الاستخدام في B7) — أي أن البنية الحالية توصّف عنصرًا قادرًا على إنشاء مرساة الثقة قبل أي حالة UNIT.
- `ARCHITECTURE_FREEZE.md:159` يجمد "Ed25519 node identity (V2)" ويصيغ HMAC كقراءة إرثية فقط خلال نافذة الإهمال — **لا يوجد بند تجميد يفرض `.unit`-HMAC**.
- `GRPC_PACKAGE_SIGNING_KEY` مطلوب إلزاميًا في الإقلاع الإنتاجي على كل عقدة (`src-tauri/src/infrastructure/security/mod.rs:123-140` عبر `validate_production_security_environment()` المستدعى من `src-tauri/src/main.rs:13`) — تحقق عمومي غير متمايز عن الوظيفة الفعلية.

# 3. بيان المشكلة (Problem Statement)

1. **سر مشترك أسطوري مطلوب للتحقق**: UNIT لا توقّع HMAC في الإنتاج (المسار الوحيد للتَوقيع هو تصدير `.unit` على WILAYA — `import_export.rs:817`)، لكنها تحتاج المفتاح نفسه للتحقق بسبب تناظر HMAC.
2. **إلزام إقلاعي غير وظيفي**: كل عقدة إنتاجية (WILAYA وUNIT) تُشترط لها عائلة HMAC الكاملة قبل بدء العملية، بينما كل تصدير الإنتاج (4 حزم بيانات) أصبح V2 حصريًا منذ Commit ④b (RFC:449-451).
3. **مبرر الاستثناء قابل للإبطال**: فرضية "لا مرساة ثقة عند التزويد" لم تعد قائمة — يوجد `install_wilaya_certificate` مرساةً جذريةً (Root-signed) تعمل على عقدة فارغة الهوية.
4. **الأثر الأمني**: تسريب مفتاح HMAC = تزوير `.unit` على مستوى الأسطول كله؛ العزل المطلوب (RFC:355 — الاستيلاء الفيزيائي على UNIT لا يكشف إلا هوية تلك العقدة) لا يتحقق مع سر مشترك.

# 4. البنية القائمة (Existing Architecture)

## 4.1 المُنتِج `.unit` (WILAYA)

- `export_unit_node_package` — `src-tauri/src/commands/import_export.rs:752-830`
  - التخويل: `authorize_command(&state, Action::ManageUnits, None)` (`:758-759`) — Wilaya+AdminOnly (RFC Rule N).
  - البيانات: `UnitNodePackage { unit, user (username/password_hash/role) }` (`:784-790`).
  - المصدر: `resolve_export_source_node_id(executor, &settings)` (`:794`).
  - الوصف: `signature_version: None`، `issuer_identity_id: None`، `package_sequence: None`، `signing_key_id: resolve_active_signing_key_id()` (`:801-810`).
  - التوقيع: `HmacPackageSigner` عبر `build_encrypted_stream_path` (`:814-819`) — توقيع HMAC بـ`GRPC_PACKAGE_SIGNING_KEY`.
  - التشفير: `state.crypto_port` — `AgeFileEncryptionProvider` (age X25519 بمفتاح App الخاص بالمُصدِّر) — **بدون تغيير في هذا القرار**.
  - **الواضح**: المصدر WILAYA هو عقدة مزوّدة بهوية Ed25519 نشطة (بعد إقلاعها) — نفس البنية التي يستخدمها `IdentitySignedExportService::export_v2_package` (`src-tauri/src/application/services/identity_signed_export_service.rs:62-134`: `NodeIdentityResolver::resolve_local_signer`، `sync_issuer_sequence_state`، `Ed25519PackageSigner`، `signature_version = Some(2)`, `issuer_identity_id`، `package_sequence`). أي أن البنية غير المتماثلة **موجودة وقيد الاستخدام** لحزم أخرى؛ النقل هو اختيار واصف الحزمة فقط.

## 4.2 المستهلك `.unit` (UNIT جديدة)

- `import_unit_node_package` — `src-tauri/src/commands/import_export.rs:454-506`
  1. وضع الإعداد: `is_setup_mode()` = `!settings.configured` (`settings_service.rs:67-69`).
  2. التخويل: anonymous `system_bootstrap` في وضع الإعداد؛ وإلا `Action::AdminOnly` (`:463-477`).
  3. فك التشفير: `read_unit_node_package_from_file(path, &state.crypto_port)` (`:486`) → `decrypt_encrypted_file_to_temp` ثم `SerdeJsonSyncPackageDeserializer::unit_node_package_from_reader` (`encrypted_package_reader.rs:135-146`).
  4. التحقق (SEC-003-05-D): `validate_unit_package_security_requirements` — وجود توقيع غير فارغ + `source_node_id` غير فارغ (`:140-154`) — ثم **داخل المـdeserializer يشتغل مسار التحقق HMAC** (`package_deserializer.rs:123-227`): trusted-signer → key-id مقبول → غير متقادم → `HmacPackageSigner.verify`.
  5. التطبيق: `NodePackageService::import_unit_node_package` (`node_package_service.rs`) — يكتب `settings` (unit name/wilaya) + مستخدم (`upsert_raw_user` بربط `node_id = unit.code`) + وحدة (`upsert_raw_unit`) + ربط `unit.user_id` — داخل `AuditTxService`.
  - **حيث يلزم HMAC**: تحديدًا خطوة التحقق في المـdeserializer (V1). لا يوجد أي استخدام HMAC في باقي المسار.

## 4.3 مرساة الثقة — `install_wilaya_certificate`

- التسجيل: `src-tauri/src/commands/identity.rs:386-412` — أمر Tauri مسجّل (`commands/registry.rs`)، **بدون `authorize_command`** (ما قبل المصادقة، موثّق في عقد الواجهة: `src/lib/contracts/identity.contract.ts:143-152` — "PRE-AUTH").
- المتطلبات: DB مفتوح فقط (`state.get_db()`)؛ لا جلسة، لا حالة UNIT، لا `NodeKeyStore`.
- الخدمة: `IdentityTrustAnchorService::install_wilaya_certificate` (`src-tauri/src/application/services/identity_trust_anchor_service.rs:58-108`):
  - `require_signed` → `subject_type == Wilaya` → `status == Active` → `issuer_identity_id.is_none()` (يصدرها Root فقط).
  - **تحقق توقيع الجذر**: `resolve_root_public_key()` + `Ed25519SignatureVerifier.verify_certificate(signed_cert, &root_public_key, signature)` (`:77-98`).
  - Idempotency: شهادة مطابقة حرفيًا (`is_identical_to`) → `AlreadyInstalled` بلا كتابة؛ مرساة ACTIVE مختلفة → **Fail-Closed** (`:100-107`).
  - الكتابة: `identity_store().upsert(signed_cert, now)` — مخزن الهويات هو مصدر الحقيقة (Single Source of Truth للثقة).
- **قابلية التشغيل قبل `.unit`**: مثبتة — لا اعتماد على `settings` ولا `units` ولا bootstrap state. مقابل ذلك، **هوية UNIT (CSR) تعتمد على `.unit`**: `resolve_local_unit_subject_id` (`identity_provisioning_service.rs:311-322`) يقرأ `settings.get_first_unit_code()` وإلا يرفض: *"No unit configured on this node; import the base node package first"*. لذا: المرساة قبل `.unit` ممكنة، الهوية بعد `.unit` حتمية — وهذا ترتيب سليم.

## 4.4 مفتاح Root العام

- `src-tauri/src/infrastructure/identity/root_public_key.rs`:
  - الأولوية: `GRPC_ROOT_PUBLIC_KEY` (Base64، 32 بايت بالضبط) → في Debug: مفتاح DEV مدمج (RFC 8032 TEST-1) مع تحذير → وإلا التثبيت المدمج `PROD_ROOT_PUBLIC_KEY` (`:103-133`).
  - **`PROD_ROOT_PUBLIC_KEY` هو مفتاح جذر الإنتاج **المعتمد** (A44-06 — سجل الاعتماد: `docs/security/A44-06-production-root-key-certification.md`، 2026-08-15): مطابق حرفيًا لمراسم `root-signer init` المنفَّذة (بصمة SHA-256 `7b38f2e1584353f9a2bf4fcc578345db3660208fecfd4c042d4efe6270072f20`). في Release + غياب المتغير → الثقة بالمثبّت المعتمد (لم يعد فشلًا مغلقًا لناقل TEST-2). يُرفض أي مفتاح TEST من البيئة أو المثبّت في Release (`:185-198`).
  - التحليل **كسول** (عند finalize/install) وليس في `validate_production_security_environment` (`:20-22`) — العقد التي لا تستخدم الإقلاع دون اتصال غير متأثرة.
  - التوزيع موثّق كقناة معتمدة: `docs/runbooks/security-production-keys.md:44-76` و`docs/runbooks/admin-bootstrap.md:26-30` — مفتاح عام فقط على كل عقدة (public، لا سرية).
- **تصنيف**: مفتاح الإنتاج الجاهزية — **عائق حاجز (BLOCKING PREREQUISITE)** لجاهزية Trust-First الإنتاجية (انظر §9.4).

## 4.5 التحقق V2 القياسي (المسار العادي)

- `SyncPackageIdentityVerificationService::verify_v2_signature` (`src-tauri/src/application/services/sync_package_identity_verification_service.rs:72-130`):
  - `signature_version == 2` → توقيع حاضر → `issuer_identity_id` حاضر → الشهادة من `identity_store().get_by_identity_id` → **SEC-003-01** (`:42-65`): `subject_type == Wilaya` + `status == Active` + غير منتهية (`not_after > now`) → تحقق Ed25519 بمفتاح الشهادة العام على `canonical_bytes`.
- ترتيب الحرس في `run_import_pipeline` (`import_export.rs:1260-1335`): SEC-003-02 (kind-aware، V2 إجباري للأنواع الحرجة) → Audit start → داخل المعاملة: `verify_v2_signature` → Transport Guard `(issuer, package_sequence)` → ... → ValidationGate → الطفرات.

## 4.6 تجاوز المسار العادي في الإقلاع

- `.unit` يتجاوز `run_import_pipeline` لأن: لا جلسة (بلا مصادقة في وضع الإعداد)، لا حالة نقل (لا ledger)، لا حاجة لـTransport Guard (حزمة واحدة)، ولأن `run_import_pipeline` يشترط `Action::*` وsession. **ما يجب إعادة إنتاجه في مسار V2 للإعداد**: SEC-003-01 (مرساة نشطة) + SEC-003-02 (kind policy) + فحص توقيع Ed25519. ما لا يمكنه الوجود بعد: جلسة، ledger تسلسلي (يبدأ بعد التزويد — أول `package_sequence` لحزم WILAYA يلي الإقلاع). **يظل `.unit` مسار استيراد خاصًا** حتى بعد V2 — هذا القرار لا ينقله إلى المسار العادي.

## 4.7 نافذة V1 للبيانات

- `DATA_PACKAGE_KINDS` (`import_export.rs:121-129`): products/daily_report/monthly_summary/stock_movements — استيراد فقط (لا تصدير V1 إطلاقًا؛ التصدير V2 حصريًا عبر `IdentitySignedExportService`).
- `SECURITY_CRITICAL_KINDS` (`:115-119`): identity_access/trust/registry — V2 إجباري، V1 مرفوض (دوران/تزوير).
- مدة النافذة **غير مقررة** (RFC:476 — "طول نافذة إهمال HMAC… المدة الدقيقة للانتهاء") — قرار مفتوح مؤكد في هذه الوثيقة (§17).
- إغلاق النافذة **مستقل** عن نقل `.unit` (قراران منفصلان — §17).

---

# 5. النتائج الجنائية (Forensic Findings)

1. **لا توجد علاقة بين المفتاحين**: `GRPC_APP_KEY` (age X25519، سرية) و`GRPC_PACKAGE_SIGNING_KEY` (HMAC-SHA256 32 بايت، أصالة إرثية) — مصدران مختلفان، لا اشتقاق، لا جذر مشترك (تحقق سابق؛ يعاد تأكيده هنا: `security/mod.rs:220-269` مقابل `appkey_store.rs`/`file_encryption.rs`).
2. **`.unit` هو موقع الإنتاج الوحيد لتوقيع HMAC** (`import_export.rs:817`؛ كل الحزم الأخرى V2).
3. **UNIT لا توقّع HMAC أبدًا في الإنتاج**؛ حاجتها للمفتاح هي فقط للتحقق (تناظر HMAC).
4. **`install_wilaya_certificate` يزيل الدائرة**: مرساة موثوقة (Root-verifiable) على عقدة فارغة، بدون أي سر خاص، قبل `.unit`.
5. **البنية غير المتماثلة موجودة وجاهزة للمنتِج**: `export_v2_package` يستخدم هوية WILAYA النشطة + ledger تسلسلي + Ed25519 — نفس المركبات اللازمة لـ`.unit` V2.
6. **التحقق V2 موجود وجاهز للمستهلك**: `verify_v2_signature` + SEC-003-01 قابلة لإعادة الاستخدام في مسار إعداد `.unit` (تحتاج فقط المرساة المثبتة مسبقًا).
7. **التجميد لا يعارض**: Freeze §2.7:159 يصيغ HMAC كقراءة إرثية؛ لا بند يجمد `.unit`-HMAC؛ النقل إلى V2 يطابق التجميد.
8. **`GRPC_PACKAGE_SIGNING_KEY` إلزام إقلاع عمومي غير وظيفي** (`security/mod.rs:123-140`) — تصنيف E/D (تحقق عمومي متعجل يفوق الحاجة الوظيفية).
9. **التبعيات المخفية مصنفة** (§16 من قائمة المطلوب): لا رمز ميت؛ كل مراجع HMAC حية ضمن نطاق V1.
10. **المفتاح Root الإنتاجي**: كان غير معتمد (ناقل TEST-2) وقت إعداد هذا التحليل — **اعتُمد لاحقًا** (A44-06، 2026-08-15 — سجل الاعتماد: `docs/security/A44-06-production-root-key-certification.md`).

# 6. القرار (Decision)

> **مقترح — للموافقة**: اعتماد نقل `.unit` إلى التوقيع V2/Ed25519 عبر ترتيب **Trust-First**:
> تركيب شهادة WILAYA الموقّعة من Root كمرساة ثقة محلية **قبل** استيراد `.unit`، والتحقق من `.unit` V2 بالمرساة المثبتة، ثم استكمال الإقلاع عبر B6-A (CSR) — وتقاعد عائلة `GRPC_PACKAGE_SIGNING_KEY`/HMAC/trusted-signers بالكامل بعد إغلاق نافذة V1.
>
> القرار **لا يمنح** أي تفويض تنفيذي؛ ينشئ عقد تنفيذ (implementation contract) للخطوات اللاحقة بعد موافقة المالك وعملية RFC-to-ADR.

## 6.1 حالة عملية RFC-to-ADR (RFC-to-ADR Process Status)

وفق `ARCHITECTURE_FREEZE.md` Section 4، تعديل أي عقد مجمَّد (و`§3.10`/`§3.12` من RFC الهوية عقود مجمَّدة ضمن نطاقه المعلن) يتطلب: RFC → مراجعة → **اعتماد وإنشاء ADR بحالة Accepted** (Step 3) → تنفيذ (Step 4).

| البند | الحالة | الملاحظة |
|-------|--------|----------|
| تعديل RFC §3.10 (استثناء `.unit` HMAC) | **NOT AUTHORIZED — NOT AMENDED** | يبقى النص الحالي ساريًا حتى اعتماد المالك لهذا القرار (Step 3) |
| تعديل RFC §3.12 D2 (ترتيب تسليم الشهادة) | **NOT AUTHORIZED — NOT AMENDED** | نفس السبب |
| إحالة ADR-0008 | **NOT AUTHORIZED — NOT SUPERSEDED** | تُنفَّذ مع إغلاق نطاق V1 (قرار منفصل) |
| تحديث mirror في ADR-0038 | **NOT AUTHORIZED — NOT UPDATED** | يتبع تعديل RFC |
| تحديث وثيقة التجميد (Freeze) | **NOT AUTHORIZED — NOT UPDATED** | Step 3 من عملية التجميد، بعد الاعتماد |
| حالة هذه الوثيقة (ADR-0044) | **PROPOSED — Decision for Owner Approval** | ليست Accepted؛ لا تُدوَّن في الفهرس إلا بهذه الحالة |

**الخلاصة**: هذه البوابة **بوابة تسجيل قرار فقط**؛ لا تُعدَّل النصوص المعيارية (RFC/Freeze/ADR-0038/ADR-0008) قبل اعتماد المالك واستكمال Step 3.

# 7. تسلسل إقلاع Trust-First (Trust-First Bootstrap Sequence)

```
S0  عقدة UNIT جديدة (لا إعدادات، لا هوية، لا مرساة)
S1  Security Setup: فتح قاعدة البيانات بمفتاح App الخاص بالعقدة (App Key حاضرة — التشفير دون تغيير)
S1.5 تحقق تكوين Root: GRPC_ROOT_PUBLIC_KEY (أو تثبيت معتمد) صالح وغير TEST (إلا في Debug)
S1.6 operator: install_wilaya_certificate(root-signed WILAYA cert file)   ← مرساة الثقة قبل .unit
      - تحقق: توقيع Root + WILAYA + ACTIVE + issuer None + idempotent + fail-closed على التعارض
S2   operator: import_unit_node_package (.unit V2)
      - فك تشفير age بمفتاح App → SEC-003-02 (.unit kind policy) → SEC-003-01 (المرساة ACTIVE)
        → تحقق Ed25519 بمفتاح WILAYA العام من الشهادة المثبتة → تطبيق setup-mode
S3   حالة UNIT موجودة (settings + unit + user محلي)
S4   B6-A: begin_unit_provision (CSR — subject_id من settings.unit_code)
      → WILAYA sign_unit_identity_request → finalize_unit_provision → هوية UNIT ACTIVE
S5   تبادل حزم V2 عادي (لا HMAC)
S6   مصادقة/تفويض عادي (مسارات B6-A)
```

> **مسار بديل (2026-08-15 — Amendment: Packaged-Identity Bootstrap):** عندما تُصدَّر
> `.unit` بواسطة `export_unit_node_package` الجديدة، تُحسم الخطوة S4 **داخل الحزمة نفسها**:
> WILAYA تولّد مفتاح UNIT Ed25519 في الذاكرة وتضمّن الشهادة الموقعة + المفتاح السري داخل
> `.unit` المشفّرة (نفس عقد §8). عند الاستيراد تُثبَّت المفاتيح والهوية **داخل معاملة
> التدقيق نفسها** (`install_node_key_matching` + `install_unit_identity_on_executor`)،
> فتصبح العقدة `UnitActive` فورًا دون مراسم CSR منفصلة. **إعادة التصدير = رفض** (حارس
> ACTIVE المكرّر في `sign_unit_bootstrap_request`). المسار القديم (CSR) يبقى ساريًا
> ودون تغيير. التفاصيل المعيارية في §8.3.

# 8. عقد `.unit` V2 (`.unit` V2 Contract)

| الحقل | القيمة المقترحة |
|-------|-----------------|
| `signature_version` | `Some(2)` (Ed25519) |
| `issuer_identity_id` | هوية WILAYA النشطة للمُصدِّر |
| `package_sequence` | `Some(1)` أو بداية ledger WILAYA للمرسل (يُقرر في التطبيق — انظر Open Decisions §30) |
| `signing_key_id` | `signer.public_key_hex()` (نمط `export_v2_package`) |
| `integrity_hash` | حاضر (SHA-256 canonical — ADR-0009) |
| `source_node_id` | غير فارغ (يبقى شرط SEC-003-05-D) |
| `role` (مستخدم الحزمة) | **NORMATIVE: `User` فقط** — لا `Admin` (تطبيق RFC:490 «لا ADMIN على عقدة UNIT إطلاقًا»؛ مسار إدارة ADMIN الحصري هو B8). التنفيذ الحالي في `validate_unit_node_role` (`node_package_service.rs:16-25`) يقبل `Admin` من الحزمة — يُقيَّد في مسار V2 ولا يُترك على الحال |
| التشفير | age X25519 بمفتاح App الخاص بالمُصدِّر — **بدون تغيير** |
| التسلسل/إعادة اللعب | idempotency موجودة (upsert)؛ يضاف فحص `package_id` مسجل في مسار الإعداد (مطلب تطبيقي) — يمنع إعادة الإرسال وإعادة الاستيراد التي تُدخل هوية مستخدم جديدة (السلوك الحالي: `INSERT OR REPLACE` بمفتاح `id` جديد في `users.rs:203-217` يعيد استبدال صف المستخدم بمُعرّف جديد عند كل استيراد) |

## 8.1 ربط العقدة الهدف (Target-Node Binding) — **NORMATIVE**

- **الوضع الحالي**: `UnitNodePackage { unit, user }` (`models/unit.rs`) لا يحمل أي مُعرّف جهاز؛ لا يوجد أي ارتباط مشفّر أو دلالي بين `.unit` والعقدة الفعلية. الاستيراد يكتب الحمولة كما هي (`node_package_service.rs`).
- **التصنيف**: **ACCEPTABLE BY DESIGN** — الارتباط في مرحلة الإقلاع **مستحيل تشفيريًا** (العقدة بلا هوية/جهاز/سر مميز قبل `.unit`؛ offline-first بلا قناة تحقق). الحماية المقبولة:
  1. **مراسم المشغّل (OPERATIONAL)**: المشغّل هو من يحمل الملف المادي إلى العقدة المقصودة؛ شاشة الاستيراد **تعرض تأكيدًا** (unit.code + unit.name + wilaya_code) ويؤكد المشغّل المطابقة قبل التطبيق — يُدرج كخطوة إجبارية في مسار V2.
  2. **مخاطر النسخ المزدوج (clone)**: استيراد نفس `.unit` في عقدتين يخلق هويتي عقدة متطابقتين (unit.code نفسه) → تباعد بيانات محتمل؛ لا يُمنع تشفيريًا في الإقلاع؛ الحدّ هو المراسم + حوكمة الأسطول (WILAYA تُصدر `.unit` واحدًا لكل code). **OPEN DECISION**: آليات ربط مستقبلية (مُعرّف جهاز مختوم من المشغّل، أو تشفير المستلم الحصري App Key) — خارج نطاق هذا القرار.
  3. `.unit` V2 لا يقبل على عقدة **مُهيَّأة** إلا بجلسة AdminOnly (السلوك القائم `import_export.rs:463-477`) — لا استبدال عشوائي لحالة عقدة عاملة.
- هذه الفئة منفصلة عن فجوة `identity_access` غير المربوطة بالعقدة المستهدفة (§12.1) — لا تخلط بينهما.

## 8.2 المتطلبات الأمنية الملزمة (Normative Security Invariants)

هذه قائمة موحدة بالثوابت الأمنية المعيارية للبنية الهدف — أي تنفيذ لاحق يخالفها يُعد انتهاكًا حوكميًا (NORMATIVE):

1. عقدة UNIT جديدة **لا تملك أبدًا**: مفتاح WILAYA الخاص، `GRPC_PACKAGE_SIGNING_KEY`، `.adminkey`، أو أي سر توقيع أسطوي مشترك.
2. Root يوقّع هويات WILAYA **فقط** (لا UNIT، لا ADMIN، لا حزم — RFC:184-185، Invariant 5).
3. UNIT تتلقى **مادة Root العامة فقط**؛ لا مادة Root خاصة بأي شكل.
4. مرساة ثقة WILAYA تُثبَّت **قبل** استيراد `.unit`.
5. تُقبل مرساة WILAYA **ACTIVE واحدة فقط** على العقدة.
6. تركيب مرساة متعارضة (ACTIVE مختلفة) **يفشل مغلقًا** — لا استبدال صامت.
7. تحقق `.unit` V2 يفرض: مُصدِّر ACTIVE + subject_type WILAYA + شهادة غير منتهية + توقيع Ed25519 سليم + سياسة kind صحيحة (SEC-003-02) + **رفض WILAYA المتقاطعة** (المُصدِّر هو المرساة المثبتة في الإقلاع).
8. **لا يُدخل أي مسار تخفيض إلى V1** في تصميم `.unit` V2 — لا قبول لـ`.unit` غير موقّعة/مخفَّضة في مسار الإعداد.
9. مستخدم `.unit` من دور **User فقط** (لا Admin — RFC:490).
10. توفير ADMIN يبقى حصريًا عبر B8 (لا مسار موازٍ).
11. تشفير الحزم يبقى age/App Key (البنية الهدف لا تغيّره).
12. التوقيع والتشفير مجالات تشفيرية منفصلة (Ed25519 ≠ age/App Key).
13. اختراق UNIT لا يكشف قدرة توقيع أسطوية (عزل RFC:355).
14. استبدال المرساة لا يحدث أبدًا بصمت (fail-closed + إعادة تهيئة).
15. التراجع ليس آلية تخفيض في مكانه (rollback = re-provisioning فقط).

## 8.3 عقد الهوية المضمّنة (Packaged-Identity Contract) — **NORMATIVE (2026-08-15)**

هذا القسم مُلزم لمسار الهوية المضمّنة الجديد (Amendment 2026-08-15) ويفسِّر §8
للحقول المضافة `unit_certificate` / `unit_private_key` في `UnitNodePackage`
(`models/unit.rs`):

| القاعدة | القيمة الملزمة |
|---------|----------------|
| الشكل | الحقلان اختياريان مع `#[serde(default, skip_serializing_if = "Option::is_none")]` — التوافق الرجعي مضمون: `.unit` القديمة (None/None) تُستورد دون تغيير |
| كلاهما-أو-لا شيء | يجب أن يحضر الحقلان معًا؛ الحضور الأحادي (شهادة فقط أو مفتاح فقط) = رفض |
| طول المفتاح | 32 بايت بالضبط (Ed25519) — غير ذلك رفض |
| التحقق قبل الكتابة | (1) الشهادة موقّعة؛ (2) subject UNIT؛ (3) `subject_id` == وحدة الحزمة؛ (4) ACTIVE؛ (5) `algorithm_version == 2`؛ (6) `issuer_identity_id` == مُصدِّر الحزمة (المُصادَق عليه بـ verify_v2_signature + B8 مقابل المرساة المثبتة)؛ (7) المفتاح السري يشتق `public_key` الشهادة |
| النقل | حصرًا داخل `.unit` المشفّرة (App Key). **لا يُكتب أبدًا** إلى `NodeKeyStore` الخاص بـ WILAYA |
| إعادة التصدير | **رفض** — حارس ACTIVE المكرّر في `sign_unit_bootstrap_request` يمنع إصدارًا ثانيًا لهوية UNIT نشطة |
| التثبيت على UNIT | داخل معاملة التدقيق: (1) استيراد القاعدة (settings/user/unit)؛ (2) `install_node_key_matching` (غائب→كتابة، مطابق→لا شيء، مختلف→رفض بلا استبدال، تالف→فشل مغلق)؛ (3) `install_unit_identity_on_executor` (نفس نواة التحقق المشتركة لـ `finalize_unit_provision` + شرط `algorithm_version`) |
| نموذج الانهيار | المسموح: «مفتاح حاضر + هوية غائبة» → إعادة استيراد نفس الحزمة تكمل التثبيت؛ الممنوع: «هوية حاضرة + مفتاح غائب» |
| المسار القديم | CSR (`begin_unit_provision`/`sign_unit_identity_request`/`finalize_unit_provision`) والاسترداد والدوران دون تغيير |

الحدود المعيارية §8.1/§8.2 وRoot Trust Model §9 وApp Key §12 تُطبق كما هي؛
المفتاح المضمّن هو مفتاح **UNIT** (يشبه مفتاح عقدة الوحدة) ويبقى مشمولًا بقاعدة
§8.2/1 من حيث عدم حيازة UNIT لأي سر WILAYA/Root/أسطوي.

# 9. نموذج ثقة الجذر (Root Trust Model)

## 9.1 ثابت

- Root يوقّع **فقط** هويات WILAYA (RFC:184-185؛ Invariant 5). لا يوقّع UNIT/ADMIN/الحزم.
- العقد تحمل مفتاح Root العام فقط (public) كمرساة؛ الخاص خارج أي عقدة (offline escrow).
- شهادة WILAYA الموقّعة من Root تُسلَّم عند التهيئة الفيزيائية — **خارج النطاق** (لا تُضمَّن في `.unit` — بند D2 "لا تُضمَّن أبدًا").

## 9.2 المقترح

- `install_wilaya_certificate` يقوم بالتحقق من السلسلة إلى Root: توقيع الشهادة مقابل `resolve_root_public_key()` — السلسلة كاملة: **Root (مفتاح عام مدمج/بيئة) ← شهادة WILAYA ← توقيع `.unit`**.
- لا توجد مرساة ثقة ثانية؛ يظل النموذج **مرساة واحدة** كما اعتمد RFC §3.11.

## 9.3 سلوك الفشل المغلق (Fail-Closed)

- Root غير قابل للحل في Release (مفتاح TEST في البيئة أو المثبّت — لا يحدث للمثبّت المعتمد) → رفض تركيب المرساة (خطأ Configuration).
- مرساة ACTIVE مختلفة موجودة → رفض (لا استبدال صامت).
- شهادة غير موقعة/غير WILAYA/غير ACTIVE/صادرة عن جهة غير Root → رفض.

## 9.4 **BLOCKING PREREQUISITE — مفتاح Root الإنتاجي**

- **الحالة**: `PROD_ROOT_PUBLIC_KEY` **مفتاح جذر إنتاجي معتمد** (A44-06، 2026-08-15) — مراسم `root-signer init` منفَّذة خارج المستودع (2026-08-14 05:11)، المفتاح العام مطابق حرفيًا للمثبّت، التحقق المستقل: بصمة SHA-256 `7b38f2e1584353f9a2bf4fcc578345db3660208fecfd4c042d4efe6270072f20` + اشتقاق عام من السر دون كشفه. السجل الكامل: `docs/security/A44-06-production-root-key-certification.md`.
- **المطلوب قبل اعتبار Trust-First جاهزًا إنتاجيًا**: توزيع المفتاح العام المعتمد على كل العقد (WILAYA وUNIT) عبر `GRPC_ROOT_PUBLIC_KEY` أو المثبّت المدمج المعتمد (وثائق التوزيع: `security-production-keys.md:44-76` و`admin-bootstrap.md:26-30`).
- **عناصر القرار المطلوبة من المالك (OWNER DECISION REQUIRED)**: مراسم توليد/اعتماد المفتاح (ceremony)؛ القيمة المرجعية الموثوقة (authoritative key value)؛ الحفظ والوصاية (custody)؛ آلية التوزيع على العقد (distribution)؛ آلية التثبيت (pin/env mechanism)؛ سياسة الدوران (rotation policy)؛ إجراء الاستعادة (recovery procedure)؛ إجراء التحقق (verification procedure).
- **لا يُستبدل هنا** — يبقى عائقًا حاجزًا مسجلاً (قرار مالك لاحق؛ **NOT IMPLEMENTED / NOT AUTHORIZED**).
- ملاحظة: هذا العائق **قائم مسبقًا** لمسار الإقلاع دون اتصال (finalize WILAYA/rotation) بغض النظر عن هذا القرار؛ Trust-First يوسع نطاقه ليشمل عقد UNIT.

# 10. متطلبات شهادة WILAYA (Certificate Requirements)

- `subject_type == Wilaya`، `status == Active`، `issuer_identity_id.is_none()` (Root)، توقيع Root سليم (كلها مفروضة تنفيذيًا في `identity_trust_anchor_service.rs:60-98`).
- إضافة مقترحة للسياق الإنتاجي: تفعيل `not_after` (انتهاء) في التحقق — الموجود في `validate_v2_issuer_certificate` (SEC-003-01) يُستخدم في مسار الإعداد المقترح فيكون شاملاً.
- الإبطال/الاستبدال: عبر دورة حياة الشهادات (SUSPENDED/REVOKED + rotation Trust Package، RFC §3.3/§3.4) — لا يُقبل أي مسار إعداد يُدخل مسارًا موازيًا للتحقق.

## 10.1 استبدال/دوران شهادة WILAYA وRoot — **NORMATIVE + OPEN DECISION**

- **خلال الإقلاع (بين S1.6 وS2)**: مرساة خاطئة مثبتة → **لا استبدال صامت** (fail-closed على أي ACTIVE مختلفة — `identity_trust_anchor_service.rs:100-107`). الإجراء الوحيد: إعادة تهيئة العقدة (مسح + إعادة S0→S1.6). **NORMATIVE**: لا آلية "استبدال مرساة" في وضع الإعداد.
- **بعد الإقلاع**: دوران شهادة WILAYA عبر B7/RFC §3.3-3.4 (rotation Trust Packages + `finalize_wilaya_rotation`/`finalize_unit_rotation`) — مسار قائم لا يمسّه هذا القرار.
- **دوران مفتاح Root (OPEN DECISION — غير محلول)**: التحليل الحالي يدعم **مفتاح Root واحد** (`GRPC_ROOT_PUBLIC_KEY` أو التثبيت المدمج — `root_public_key.rs`)، بلا دعم تعدد جذور. دوران Root يعني: إعادة توقيع شهادة WILAYA بالجذر الجديد + إعادة تركيب المرساة على كل عقدة (مع تداخل نوافذ قبول للجذرين القديم والجديد أو قطع صريح). السياسة (الجدول الزمني، التغذية المتدرجة، توثيق النوافذ) قرار مالك مستقبلي — يُسجَّل في §26.

# 11. متطلبات التحقق في وضع الإعداد (Setup-Mode Verification Requirements)

1. `SEC-003-01` **يُعاد إنتاجه بالكامل** في مسار إعداد `.unit`: شهادة المُصدِّر موجودة في Identity Store + `Wilaya` + `Active` + غير منتهية — عبر `validate_v2_issuer_certificate` نفسها (تحتاج `executor` فقط، وهو متاح).
2. `SEC-003-02` kind policy لـ`.unit` (مثل `SECURITY_CRITICAL_KINDS`): V2 إجباري + توقيع + issuer + sequence + integrity.
3. تحقق Ed25519 على `canonical_bytes` بمفتاح WILAYA العام (نمط `verify_v2_signature`).
4. **لا تُخفَّف**: لا قبول لشهادات معلقة/مستبدلة/منتهية/خاطئة النوع/خاطئة المُصدِّر/مزوّرة الهوية — نفس المعايير الحرفية للمسار العادي.
5. لا حاجة لـTransport Guard في الإقلاع (لا ledger بعد) — يُستبدل بفحص `package_id` مسجل (idempotency + مكافحة إعادة اللعب داخل حدود الإقلاع).
6. `.unit` **يبقى** مسار استيراد خاصًا (بلا جلسة، بلا pipeline) — لا يُنقل إلى `run_import_pipeline`.

## 11.1 رفض WILAYA المتقاطعة (Cross-WILAYA Rejection) — **NORMATIVE**

- شهادة المُصدِّر تُحل عبر `issuer_identity_id → get_by_identity_id` ثم SEC-003-01 (`sync_package_identity_verification_service.rs:42-108`) — **أي** شهادة ACTIVE من نوع WILAYA في المخزن تمر تقنيًا، لذا يعتمد الحصر على:
  1. **مرساة ACTIVE واحدة** على العقدة (يفرضها `install_wilaya_certificate` — أي ACTIVE ثانية تُرفض).
  2. شهادات WILAYA القديمة تُسجَّل `SUSPENDED/REVOKED` عند الدوران (تُرفض في SEC-003-01).
- **حكم NORMATIVE لمسار الإعداد**: `.unit` V2 لا يُقبل إلا إذا كانت شهادة المُصدِّر **هي المرساة المثبتة نفسها** (get_active_by_subject_type(Wilaya) — عقدة إقلاع لا تحمل غيرها) — يمنع قبول حزمة موقّعة من WILAYA ثانية/دخيلة في مرحلة الإقلاع. لا "WILAYA متقاطعة" في الإقلاع تحت أي شرط.
- بعد الإقلاع، رفض الـWILAYA المتقاطعة يستمر عبر SEC-003-01 + قاعدة الحوكمة القائمة (لا قبول لموقّع خارجي دون اعتماد صريح — RFC §3.4).

# 12. حدود App Key (App Key Boundary)

- **التشفير دون تغيير**: age X25519 بمفتاح App الخاص بالعقدة (`file_encryption.rs`، `crypto_port`). لا يُقترح أي تعديل.
- **فصل المجالات**: التشفير (App Key) ≠ التوقيع (Ed25519) ≠ مرساة الثقة (Root/WILAYA) ≠ هوية العقدة (NodeKeyStore).
- **قرار تبادل/توزيع App Key يبقى خارج النطاق** (عملية مستقلة، مالكها معلق) — هذا القرار لا يفتحه ولا يغلق.
  - **تحديث (2026-08-15 — ADR-0041 §10 / A44-13):** تم حسم تبادل App Key
    **للهندسة الحالية** صراحةً كـ **Model C** — App Key مفتاح سرية مشترك بين
    WILAYA وأسطول الوحدات، تُوفَّر قيمته عبر `GRPC_APP_KEY` بقيمة مشغّل واحدة
    عند النشر (ADR-0041 §10). لا يُنسخ `appkey.age` بين العقد؛ لا يوجد أمر
    استيراد App Key؛ لا يوجد تشفير موجه للمستلم (recipient-targeted) الآن.
    **التشفير الموجَّه للمستلم (B5) يبقى قرارًا مستقبليًا** للحزم ما بعد
    الإقلاع فقط — لم يُنفَّذ، ولا تدّعي هذه الوثيقة تنفيذه.

## 12.1 علاقة B8 (B8 Dependency) — **SEPARATE BLOCKER**

- **الوضع**: بعد `.unit` (V1 أو V2)، تمتلك UNIT جلسة مستخدم من دور **User فقط** (WILAYA-side: `unit_service.rs:57`؛ والقيود §8 تحصرها كذلك في V2). استيراد `identity_access` (B8) يشترط جلسة `AdminOnly` على UNIT (`policies/mod.rs` — SEC-003-06-b) — أي أن **أول استيراد B8 غير قابل للوصول على عقدة إقلاع** (deadlock قائم، مثبت في التحقيقات السابقة).
- **أثر Trust-First على B8**: **لا يغيّر شيئًا** — النقل إلى V2 لا يُنشئ جلسة Admin على UNIT ولا يفتح مسار B8. لا يحسّنه ولا يزيده سوءًا.
- **الإبقاء كعائق منفصل**: B8 (آلية توفير حساب ADMIN الأول وربط العقدة المستهدفة) يحتاج **قرارًا معماريًا مستقلًا** — خياراته المسجلة (لا تُحسم هنا): (أ) إعفاء setup-mode لأول استيراد `identity_access` (بضمانات موازية لمسار `.unit`)، أو (ب) توفير الحساب الإداري الأول عبر مسار آخر بموافقة صريحة على الاستثناء من RFC:490، أو (ج) مراجعة سياسة التخويل في أول استيراد. **OPEN DECISION** — §26.
- **تحديث (2026-08-14)**: قرار مرفق **ADR-0045** (خيار A — إعفاء أول استيراد `identity_access` بضمانات package-authenticated، مع تركيب المرساة سابقًا وضوابط SEC-003) يعالج هذا البند كقرار معماري مستقل؛ **البنية OWNER RATIFIED بشروط** (بوابة اعتماد المالك)، و**اكتمل اعتمادها مع بقية قرارات B8 الفرعية** (A45-03/04/05/08/09/10) في بوابة اكتمال الاعتماد — السجل الكامل في [ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md). **التنفيذ وتعديل RFC غير مصرحين بعد**.
- التزامًا بالحدود: هذه الوثيقة لا تحل B8 ولا تعتمد أي آلية منه.

# 13. نافذة بيانات V1 (Legacy V1 Data Window)

## 13.1 قراران مستقلان (Decision A / Decision B)

```
القرار A: تقاعد .unit HMAC → V2          القرار B: إغلاق نافذة استيراد V1 للبيانات
   (موضوع هذه الوثيقة)                        (قرار مالك منفصل — تاريخ/دليل)
```

- القراران **مرتبطان لكن مستقلان**: لا يُبرَّر بقاء HMAC بوجود حزم V1، ولا يُشترط إغلاق النافذة لنقل `.unit`. كلاهما يُحسم مالكيًا وبشكل منفصل؛ تُنفَّذ إزالة `GRPC_PACKAGE_SIGNING_KEY` فقط بعد **اكتمال القرارين معًا**.

## 13.2 الحالة

| البند | الحالة |
|-------|--------|
| الأنواع المقبولة V1 | products/daily_report/monthly_summary/stock_movements (استيراد فقط) |
| تصدير V1 | لا يوجد (الكل V2 منذ Commit ④b) — CONFIRMED |
| الأنواع الحرجة V1 | مرفوض (identity_access/trust/registry — SEC-003-02) — CONFIRMED |
| المدة | **غير مقررة** (RFC:476) — OWNER DECISION REQUIRED |
| الاستقلال عن `.unit` | نعم — CONFIRMED |

## 13.3 قرارات مطلوبة

- تحديد تاريخ/شرط إغلاق النافذة (OWNER DECISION REQUIRED).
- معايير الدليل قبل الإغلاق: كل الأسطول على إصدار داعم V2، لا حزم V1 قيد التداول (مسح المستودعات)، تشغيل E2E بدون أي V1 — CONFIRMED كمعايير مقترحة.
- بعد الإغلاق: يُحذف فرع V1 في `package_deserializer.rs` + `DATA_PACKAGE_KINDS` يصبح V2-only — **NOT IMPLEMENTED / NOT AUTHORIZED**.

# 14. خطة تقاعد HMAC (HMAC Retirement Plan)

**حدود التقاعد (Retirement Boundary)**: `GRPC_PACKAGE_SIGNING_KEY` **مُعيَّن للتقاعد (designated for retirement)** بعد اعتماد نقل `.unit` إلى V2 **و** إغلاق نافذة استيراد V1 للبيانات — وليس محذوفًا اليوم. الحالة الحالية لكل عنصر: `NOT IMPLEMENTED / NOT AUTHORIZED`. الجدول التالي يحدد **نطاق العمل المستقبلي** (future implementation scope)، لا عملًا جاريًا:

| الصنف | العنصر | مُعيَّن للتقاعد بعد النقل+الإغلاق |
|-------|--------|:---:|
| مفتاح | `GRPC_PACKAGE_SIGNING_KEY` | نعم |
| مكوّن | `HmacPackageSigner` / `HmacPackageVerifier` | نعم |
| دالة | `resolve_package_signing_key_32_impl` / `resolve_package_signing_key_32` | نعم |
| واصف | `resolve_active_signing_key_id` (GRPC_ACTIVE_SIGNING_KEY_ID) | نعم |
| تحقق | فرع V1 في `package_deserializer.rs:123-227` (trusted signer/key-id/deprecated/HMAC) | نعم |
| تحقق | فرع V1 لتوقيع `.unit` (SEC-003-05-D يتقادم لصالح V2) | نعم |
| توقيع | توقيع V1 في `export_unit_node_package` (`import_export.rs:817`) | نعم |
| إعداد | `GRPC_TRUSTED_SIGNER_IDS` / `GRPC_ENFORCE_TRUSTED_SIGNERS` | نعم |
| إعداد | `GRPC_ACCEPTED_SIGNING_KEY_IDS` / `GRPC_DEPRECATED_SIGNING_KEY_IDS` | نعم |
| أدوات | `is_trusted_signer` / `resolve_accepted_verification_key_ids` / `is_signing_key_deprecated` / `resolve_trusted_signer_ids` | نعم |
| إقلاع | بنود HMAC في `validate_production_security_environment` (`security/mod.rs:123-171`) | نعم |
| توثيق | `security-production-keys.md` §5، README، runbooks | نعم |
| اختبارات | Fixtures/اختبارات V1 | نعم |

> **لا تُنفَّذ أي إزالة في هذه الخطوة.** هذه قائمة عقد تنفيذ مستقبلي (§29).

# 15. تقاعد الإعدادات (Configuration Retirement)

- **المبدأ**: `GRPC_PACKAGE_SIGNING_KEY` يجب ألا يبقى شرط إقلاع إنتاجيًا عموميًا لمجرد وجود HMAC إرثي.
- **بعد النقل + إغلاق النافذة**: لا تحقق من مفتاح HMAC، لا trusted-signer config، لا اعتماد إقلاع على HMAC إطلاقًا.
- **قبل اكتمال الهجرة**: يُحفظ السلوك التوافقي الحالي ما لم يُعتمد إغلاق منفصل (قرار مالك).
- بديل مؤقت (اختياري، قرار مالك): جعل بند HMAC في الإقلاع مشروطًا بوجود `GRPC_ENV` ومستقلًا عن عقد UNIT — **لا يُقترح كقرار هنا**؛ يُسجَّل كخيار في Open Decisions.

# 16. خطة الهجرة (Migration Plan)

1. اعتماد هذا القرار (RFC-to-ADR) + تعديل RFC §3.10/§3.12 وADR-0038 mirror.
2. عائق Root: اعتماد مفتاح Root عام إنتاجي وتوزيعه (قرار مالك/مراسم).
3. تنفيذ: `.unit` V2 — منتِج (تصدير WILAYA بـ`export_v2_package` نمط) + مستهلك (مسار إعداد V2 مع SEC-003-01/02 + قيود §8).
4. قرار نافذة V1 وتاريخ الإغلاق (قرار مالك منفصل).
5. إزالة عائلة HMAC (بعد الإغلاق) + إحالة ADR-0008.
6. تحديث runbooks/وثائق التشغيل + اختبار E2E.
7. **التراجع (rollback)**: لا تراجع في مكانه من حالة مزوّدة — إعادة تهيئة كاملة فقط (§20.1)؛ يُوثَّق في runbook ولا يُدعم استيراد تراجعي لـ`.unit` قديم.
8. **B8**: لا يُدمج أي حل B8 في هذا القرار؛ يُعتمد قرار مستقل (§12.1، §26#8) بالتوازي عند الحاجة.

# 17. التوافق الرجعي (Backward Compatibility)

- عقد UNIT قديمة (مزوّدة بـ`.unit` HMAC) تبقى تعمل: تحقق V1 يبقى خلال نافذة الإهمال؛ لا إبطال لمراسي قائمة.
- `.unit` V2 لا يتوافق مع إصدارات UNIT القديمة (المستهلك القديم لا يتحقق V2) — فترة انتقالية تتطلب توليد `.unit` بالإصدارين (قرار تشغيلي، يُسجَّل في Open Decisions) أو توافقًا زمنيًا للأسطول (كل UNIT تُحدَّث قبل إصدار `.unit` V2).
- توزيع `.unit` V2 مشروط بوجود المرساة: لا يقبل إلا بعد `install_wilaya_certificate` — خطأ واضح يرشد المشغل للترتيب.

# 18. استراتيجية الطرح (Rollout Strategy)

- **مرحلي بحسب العقد**: WILAYA أولاً (مزوّدة بهوية + مفتاح Root)، ثم عقد UNIT عبر إجراء re-provisioning أو بث تحديث عميل يدعم V2.
- **التحقق من الجاهزية**: مفتاح Root معتمد على كل العقد قبل أول `.unit` V2.
- **مراقبة**: كل فشل تحقق V2 في مسار الإعداد يسجَّل (audit + logs) بنفس مستوى خطورة فشل المسار العادي.

# 19. أنماط الفشل (Failure Modes)

| النمط | السلوك المطلوب |
|-------|----------------|
| `.unit` V2 بلا مرساة مثبتة | رفض عند SEC-003-01 (مُصدِّر غير موجود) — رسالة إرشادية: ثبّت شهادة WILAYA أولاً |
| مرساة منتهية/معلقة | رفض (validate_v2_issuer_certificate) |
| `.unit` V1 بعد إغلاق النافذة | رفض (kind policy) |
| Root غير معتمد (Release) | رفض تركيب المرساة (فشل مغلق) — لا fallback |
| مفتاح App خاطئ | فشل فك التشفير قبل أي تحقق/كتابة (كما اليوم) |
| إعادة إرسال `.unit` | idempotency + فحص `package_id` مسجل → رفض تكرار |
| `.unit` V2 مزوّر | فشل تحقق Ed25519 |

# 20. الاستعادة / إعادة التزويد (Recovery / Re-provisioning)

- إعادة تزويد عقدة: مسح قاعدة البيانات + إعادة S0→S4 (بمرساة جديدة إن لزم).
- مرساة خاطئة مثبتة عن طريق الخطأ: لا استبدال صامت — fail-closed؛ الإجراء: re-provision مع إعادة تركيب الشهادة الصحيحة (قرار تشغيلي يُوثَّق في runbook).
- تلف `appkey.age`: يتبع إجراء ADR-0041 الحالي (استعادة من نسخة احتياطية/إعادة تزويد).

## 20.1 نقاط الاستعادة بين خطوات الإقلاع — **NORMATIVE + OPERATIONAL**

| النقطة | الحالة بعد الفشل | الاستعادة |
|--------|------------------|-----------|
| بعد S1 (App Key فقط) | لا حالة هوية/إعداد | إعادة المحاولة من S1؛ لا بقايا |
| بعد S1.6 (مرساة مثبتة، `.unit` لم يُستورد) | مرساة ACTIVE + لا settings/units | إعادة استيراد `.unit` (المرساة مطابقة → `AlreadyInstalled`)؛ مرساة خاطئة → fail-closed → **إعادة تهيئة كاملة** (لا استبدال) |
| بعد S2 (حالة UNIT موجودة، قبل B6-A) | settings/unit/user محليون، بلا هوية UNIT | إعادة CSR (`begin_unit_provision` — قابلة للتكرار)؛ إعادة استيراد `.unit` **ممنوعة** (package_id مسجل + العقدة لم تعد setup) — لا يُعاد استيراد الملف إلا عبر إعادة تهيئة كاملة |
| بعد S4 (هوية UNIT ACTIVE) | عقدة مزوّدة بالكامل | مسار B7 للدوران؛ لا إعادة إقلاع |

- **NORMATIVE**: لا يُقبل استيراد `.unit` ثانٍ على عقدة مهيَّأة (مغلق بالجلسة والـpackage_id)؛ التراجع من حالة مزوّدة = إعادة تهيئة (factory reset) عبر إجراء موثق، وليس استيرادًا تراجعيًا.

## 20.2 التدقيق، مراسم المشغّل، والنقل دون اتصال — **IMPLEMENTATION + OPERATIONAL**

- **التدقيق (IMPLEMENTATION)**: تركيب المرساة (`install_wilaya_certificate`) **غير مُدوَّن حاليًا** في سجل التدقيق (لا `AuditTxService` في `identity_trust_anchor_service.rs`) — مطلب تنفيذي: تدقيق كل من تركيب المرساة، استيراد `.unit` (موجود: `AuditAction::ImportNodePackage`)، خطوات الإقلاع (begin/finalize/install). يخدم P4 (Auditability).
- **مراسم المشغّل (OPERATIONAL)**: حمل ملفي الشهادة و`.unit` دون اتصال (وسائط فيزيائية)؛ التحقق من هوية الملفين (المحتوى الموقّع/المشفّر يحمي الأصالة والسرية — لا حاجة لقناة إضافية)؛ خطوة **تأكيد unit.code/name قبل التطبيق** (الربط بالعقدة — §8.1)؛ الاحتفاظ بنسخ الملفات في سجل الأسطول لدى WILAYA.
- **النقل دون اتصال**: لا يُقترح أي تغيير — الملفات الموقّعة/المشفّرة تمر بالمراسم الحالية؛ التلاعب بها يُكشف عند التحقق (Root/Ed25519/age).

## 20.3 سيناريوهات الاختراق — **NORMATIVE**

- **اختراق مفتاح WILAYA الخاص**: يقع في نموذج التهديد (RFC §3.5.1) — يمكنه تزوير `.unit` وكل حزم V2 حتى الدوران/الإبطال. الالتزام (NORMATIVE): مدار عبر دورة الحياة (B7 rotation + revocations عبر trust packages)؛ لا يقبل هذا القرار أي تخفيف دائم لهوية WILAYA المخترقة. تسجيل الشهادة المخترقة `REVOKED` يبطل أي `.unit` مستقبلي موقّع بها (SEC-003-01).
- **اختراق UNIT**: يكشف هوية UNIT فقط (عزل RFC:355) — بلا أثر أسطوي؛ لا سر أسطوي بعد التقاعد (هذا جوهر القرار).
- **خسارة مفتاح App للعقدة**: إعادة تزويد كاملة (ADR-0041) — لا استرداد للبيانات المشفّرة بالنسخ الاحتياطية إلا بالمفتاح (إجراء قائم).

# 21. نموذج التهديد (Threat Model)

| التهديد | HMAC (الحالي) | Trust-First V2 (المقترح) |
|---------|---------------|--------------------------|
| استيلاء فيزيائي على UNIT | كشف مفتاح HMAC → تزوير `.unit` أسطوي | كشف مفاتيح UNIT فقط (عزل RFC:355) |
| تسريب مفتاح HMAC | تزوير كامل نطاق V1 | غير قابل (لا سر مشترك) |
| انتحال WILAYA | جزئي (نطاق V1/trusted signers) | مستحيل (مرساة Root + Ed25519) |
| مرساة بديلة | — | مرفوضة (fail-closed على التعارض) |
| شهادة معلقة/مستبدلة/منتهية | — | مرفوضة (SEC-003-01) |
| WILAYA متقاطعة | تعتمد على قوائم | مرفوضة (نوع/هوية المُصدِّر) |
| تزوير `.unit` بمفاتيح UNIT المحلية | غير ممكن أصلاً (HMAC لا يستخدمها) | غير ممكن (لا يملك WILAYA الخاص) |
| إعادة اللعب | idempotency جزئية | idempotency + package_id مسجل |
| **نصف قطر الانفجار** | أسطول | عقدة/هوية واحدة |

# 22. مقارنة أمنية: HMAC مقابل V2 (Security Comparison)

- **السرية**: HMAC يتطلب سرًا مشتركًا أسطويًا للتحقق (انتهاك مبدأ "لا سر في التحقق")؛ V2 يتطلب مفتاحًا عامًا فقط.
- **الإثبات**: HMAC بلا إثبات ملكية؛ V2 مربوط بهوية WILAYA (owner proof) — RFC §1.2 الهدف الأصلي.
- **دورة الحياة**: HMAC بلا دوران/إبطال فعّال (توزيع يدوي لكل العقد)؛ V2 بشهادات (rotation/revocation عبر trust packages).
- **الالتزام بالتجميد**: V2 هو النمط المجمَّد (Freeze §2.7)؛ HMAC نافذة إرثية.
- **الحتمية**: Ed25519 حتمي (RFC 8032) — لا أثر على Freeze §2.5 (مؤكد RFC:454-455).

# 23. متطلبات التشغيل (Operational Runbook Requirements)

1. `security-production-keys.md`: §5 يُستبدل بإرشادات مفتاح Root العام (توزيع public فقط)؛ بند `GRPC_PACKAGE_SIGNING_KEY` يُسحب بعد الإغلاق.
2. `admin-bootstrap.md`: يُضاف ترتيب UNIT: المرساة قبل `.unit`.
3. Runbook جديد (أو قسم): "ترتيب إقلاع UNIT Trust-First" — خطوات S0–S6 مع كل أوامر Tauri.
4. Runbook استكشاف: فشل `.unit` بسبب "المُصدِّر غير موجود" → خطوة تثبيت المرساة.
5. تحديث `sync-runbook.md`/`import-failure-investigation.md` لرموز فشل V2 في مسار الإعداد.

# 24. متطلبات الاختبار/التحقق (Test / Verification Requirements)

العقد E2E المطلوب (يُكتب لاحقًا — لا يُكتب الآن):

```
S0 Fresh UNIT → S1 App Key → S1.5 Root config → S1.6 install WILAYA cert
→ S2 import V2 .unit → S3 UNIT state → S4 accounts → S5 B6-A identity
→ S6 V2 package exchange → S7 login/authz
```

يجب أن يثبت:
- لا مفتاح HMAC على UNIT (لا `GRPC_PACKAGE_SIGNING_KEY`)
- استيراد `.unit` V2 ينجح (مرساة مثبتة)
- `.unit` مزوّر يفشل / شهادة WILAYA خاطئة تفشل / منتهية تفشل / معلقة-مستبدلة تفشل / Root خاطئ يفشل / App Key خاطئ يفشل
- **WILAYA متقاطعة تفشل** (حزمة موقّعة من شهادة WILAYA غير المرساة المثبتة) (§11.1)
- **إعادة استيراد `.unit` على عقدة مهيَّأة تفشل / إعادة الإرسال تفشل** (package_id مسجل) (§8، §20.1)
- **دور `Admin` في `.unit` يُرفض** (قيود §8 — User فقط)
- إعادة اللعب/التكرار مغلقة فشلًا
- حزم V2 العادية تستمر / UNIT لا تستطيع تزوير حزم WILAYA

إضافة: اختبار وحدة أن `install_wilaya_certificate` ينجح قبل أي `.unit` (DB فارغة هويات)، واختبار مسار إعداد V2 مع/بدون مرساة.

# 25. قائمة العمل التنفيذي المستقبلي (Required Future Implementation Work)

> **NOT AUTHORIZED** — كل البنود التالية عقد تنفيذ مستقبلي يُنفَّذ فقط بعد اعتماد المالك واستكمال RFC-to-ADR (Step 3) وإزالة العوائق (§9.4، §12.1). لا شيء منها منفَّذ اليوم.

1. هجرة منتِج `.unit`: HMAC-V1 → Ed25519 V2 (واصف V2 + ledger + `export_v2_package` نمط).
2. تحقق V2 في وضع الإعداد لـ`.unit` (SEC-003-01 + SEC-003-02 + Ed25519) في المسار الخاص.
3. تركيب المرساة قبل `.unit` (ترتيب تشغيلي + أتمتة UI).
4. تحقق سلسلة Root في تركيب المرساة (موجود — يُعاد استخدامه كما هو).
5. تحقق دورة حياة الشهادة (not_after إلزامي في مسار الإعداد).
6. تغييرات واصف الحزمة/`signature_version` (V2 لـ`.unit`).
7. إزالة بنود HMAC من `validate_production_security_environment`.
8. إزالة إعدادات HMAC (env + دوال).
9. إزالة trusted-signer config + إحالة ADR-0008.
10. إغلاق نافذة استيراد V1 للبيانات (قرار منفصل).
11. هجرة الاختبارات (V1 fixtures → V2).
12. اختبار E2E bootstrap (أعلاه).
13. هجرة التوثيق/runbooks.
14. هجرة النشر (متغيرات البيئة، التوزيع).
15. معالجة التراجع (rollback) — توثيق العودة لإصدار `.unit` السابق إن لزم.

# 26. القرارات المفتوحة (Open Decisions)

| # | القرار | المالك المقترح |
|---|--------|----------------|
| 1 | إغلاق نافذة V1 للبيانات: تاريخ/شرط | Architecture / Operations |
| 2 | `package_sequence` الأول لـ`.unit` V2 (بداية ledger WILAYA) | Architecture / Security |
| 3 | فترة انتقالية مزدوجة (توليد `.unit` V1+V2) أم تزامن ترقية الأسطول | Operations |
| 4 | اعتماد مفتاح Root الإنتاجي (قيمة/مراسم/تثبيت) — **عائق حاجز** | Owner (Security) |
| 5 | توزيع App Key (خارج النطاق — مؤكد هنا كمستقل) | Owner (معلق سابقًا) |
| 6 | جعل بند HMAC الإقلاعي مشروطًا مؤقتًا (خيار بديل قبل الإغلاق) | Architecture |
| 7 | **سياسة دوران مفتاح Root** (تعدد جذور/نوافذ قبول/قطع صريح — غير مدعوم اليوم، §10.1) | Owner (Security) |
| 8 | **آلية أول استيراد B8** على عقدة إقلاع (إعفاء setup-mode / استثناء RFC:490 / مراجعة السياسة — §12.1) — قرار مرفق مقترح: **ADR-0045 خيار A** | Architecture / Security |
| 9 | **آليات ربط العقدة الهدف المستقبلية** لـ`.unit` (مُعرّف جهاز مختوم / تشفير مستلم حصري — §8.1) | Architecture |
| 10 | سلوك ملفات `.unit` القديمة أثناء الانتقال (قبول فقط عبر النافذة؛ لا خطة تحويل) — تأكيد صريح | Operations |

# 27. معايير الرفض (Rejection Criteria)

يُرفض هذا القرار (أو يُعلق) إذا ثبت:
1. تعارض ملزم في Freeze §2.7 مع إزالة HMAC من `.unit` (غير موجود حاليًا).
2. عدم قدرة `install_wilaya_certificate` على العمل على عقدة فارغة (مفند — لا يعتمد على أي حالة UNIT).
3. حاجة لا يمكن تلبيتها لمسار نقل/تسلسل في الإقلاع قبل وجود ledger (لا حاجة — حزمة واحدة + idempotency).
4. وجود سر مشترك جديد مقترح كبديل عن HMAC (مرفوض صراحةً — لا يوجد).
5. عدم إمكانية توزيع مفتاح Root العام على العقد (موثّق بالفعل كقناة معتمدة).

# 28. القرار النهائي (Final Decision)

**مقترح للموافقة** — الأدلة تدعم النقل: البنية غير المتماثلة كاملة (Root→WILAYA→Ed25519) موجودة ومستخدمة لبقية الحزم؛ مرساة الثقة قابلة للتركيب قبل `.unit` بموجودات اليوم؛ التجميد لا يعارض؛ الأمان يتحسن (إزالة السر الأسطوي الوحيد). **العائق الحاجز الوحيد**: اعتماد مفتاح Root الإنتاجي. **القرارات المفتوحة**: نافذة V1، التسلسل الأول، فترة الانتقال، توزيع App Key (مستقل)، دوران Root، آلية أول استيراد B8، ربط العقدة الهدف (§26). — **تحديث (2026-08-14)**: جميع هذه القرارات **OWNER RATIFIED** في بوابة اكتمال الاعتماد (سجل §26.6 في ADR-0045)؛ يبقى العائق الحاجز: مفتاح Root الإنتاجي (§9.4)، ويبقى التنفيذ غير مصرح به.

## 28.1 نتيجة مراجعة المالك (Owner Review Outcome — 2026-08-14)

- **الحكم**: **APPROVED WITH BLOCKERS** — البنية (`Trust-First V2/Ed25519`) معتمدة معماريًا لمراجعة المالك، مشروطة بحسم العائق الحاجز (§9.4) وبقية القرارات المفتوحة (§26).
- **لا يُصرَّح بالتنفيذ**: لا يبدأ أي عمل تنفيذي (كود/اختبارات/إعدادات) قبل (1) اعتماد المالك لهذا القرار، (2) حسم عائق مفتاح Root الإنتاجي، (3) استكمال عملية RFC-to-ADR (تعديل RFC §3.10/§3.12 وADR-0038، إحالة ADR-0008).
- **العائق المنفصل غير المغطى**: B8 (أول استيراد `identity_access` على عقدة إقلاع) يبقى عائقًا مستقلاً يحتاج قرارًا معماريًا خاصًا (§12.1) — لا يمنع اعتماد هذا القرار ولا يُحل به.

## 28.2 نتيجة بوابة حوكمة الوثائق (Documentation Governance Gate — 2026-08-14)

- **الحكم**: **APPROVED WITH BLOCKERS** (تأكيد نهائي لسجل القرار). الوثيقة الآن سجل قرار مدقَّق يجيب على: ما الذي تقرر (§6/§8/§8.2) — وما الذي بقي محجوبًا (§9.4، §12.1) — وما الذي ينتظر المالك (§26) — وما الممنوع تنفيذه (§خريطة الحالة، §6.1، §25).
- **حالة RFC-to-ADR**: RFC §3.10 و§3.12 D2 **لم يُعدَّلا**؛ ADR-0008 **لم تُحَل**؛ mirror ADR-0038 **لم يُحدَّث**؛ وثيقة التجميد **لم تتغير** — الكل `NOT AUTHORIZED` حتى اعتماد المالك (Freeze §4 Step 3) (§6.1).
- **حدود الوثيقة**: تعديلات هذه البوابة وثائقية بحتة (هذه الوثيقة + فهرس ADRs)؛ لا رمز، لا إعدادات، لا مفاتيح، لا HMAC محذوف، لا B8 معدّل، لا App Key متغير.

## 28.3 نتيجة بوابة حسم قرارات المالك (Owner Decision Resolution Gate — 2026-08-14)

- **الغرض**: تسوية مواقف القرارات المفتوحة في ADR-0044 وADR-0045 إلى حالة قرار مدقَّقة (سجل حسم واحد مرجعي في [ADR-0045 §25](docs/architecture/0045-b8-first-identity-access-import.md) — لا يُكرَّر الجدول هنا).
- **مصير قرارات ADR-0044 (A44-01..A44-13)**: لا يوجد اعتماد مالكي موثّق لأي من العناصر المعيارية المقترحة؛ مراجعات البوابات السابقة («APPROVED WITH BLOCKERS») **حكم مراجعة وليست اعتماد مالك**. المواقف المسجلة: A44-01..A44-05 = OWNER DECISION REQUIRED / NOT AUTHORIZED (مقترح معلق — تعديلات RFC/ADR-0038/ADR-0008 غير مصرح بها)؛ A44-06 = BLOCKED (مفتاح Root الإنتاجي غير معتمد — ناقل TEST-2 مثبت) — **أُلغي لاحقًا: اعتمد في 2026-08-15 (سجل: `docs/security/A44-06-production-root-key-certification.md`)**؛ A44-07..A44-10 = OWNER DECISION REQUIRED؛ A44-11/A44-12 = مقسّمة إلى حقائق مقررة (DECIDED) وسياسات معلقة (OWNER DECISION REQUIRED)؛ A44-13 = OUT OF SCOPE.
- **مصير قرارات ADR-0045 (A45-01..A45-12)**: A45-01 (خيار A) = OWNER DECISION REQUIRED؛ A45-02/04/09/10 مقسّمة إلى ضوابط قائمة (DECIDED — مفروضة في الكود الحالي أو حقائق مثبتة) وبنود جديدة مقترحة (OWNER DECISION REQUIRED)؛ A45-03/05/08 = OWNER DECISION REQUIRED؛ A45-06/07/11/12 = DECIDED (مقيدة بدليل الكود/الحوكمة، لا حاجة لقرار مالك لاحق).
- **الاتساق العابر بين ADR-0044 وADR-0045**: **CONSISTENT** — البنود الـ14 للمراجعة المتقاطعة (ترتيب المرساة، أصالة `.unit` V2، تسلسل الثقة، ربط المُصدِّر، ربط unit_code، أول package_sequence، حدود تقاعد V1، سياسة دور `.unit`، تخويل أول استيراد B8، تمييز هوية/حساب ADMIN، نطاق SEC-002، نطاق App Key، التراجع، عائق Root) — لا تعارض بينهما (سجل §25.2).
- **حالة RFC-to-ADR بعد التسوية**: RFC §3.10 و§3.12 D2 **لم يُعدَّلا**؛ ADR-0008 **لم تُحَل**؛ mirror ADR-0038 **لم يُحدَّث**؛ وثيقة التجميد **لم تتغير** — حالة إجمالية: **RFC AMENDMENT BLOCKED** (القرارات المالكية المتبقية A44-01 وA45-01 تمنع Step 3 من عملية Freeze §4).
- **حدود البوابة**: وثائقية فقط (ADR-0044، ADR-0045، فهرس ADRs إن لزم)؛ لا رمز، لا مفاتيح، لا RFC معدَّل، لا ترقية لأي بند إلى DECIDED دون دليل، ولا أي تفويض تنفيذي بأي شكل.

## 28.4 نتيجة بوابة اعتماد المالك (Owner Ratification Gate — 2026-08-14)

- **الغرض**: تسجيل الاعتماد المالكي الصريح للقرارات المتبقية في ADR-0044 وADR-0045 في سجل موحّد واحد مرجعي — [ADR-0045 §26](docs/architecture/0045-b8-first-identity-access-import.md) (سجل اعتماد المالك). لا يُكرَّر الجدول هنا.
- **ما اعتمده المالك**: **A45-01 + A45-02** — بنية خيار A (إعفاء أول استيراد `identity_access` package-authenticated ذاتي الإنهاء مع تركيب `install_wilaya_certificate` مسبقًا) ومجموعة الضوابط الإلزامية الكاملة (13 شرطًا) كما نصَّت تعليمات البوابة: "Do NOT weaken these controls". لا يمنح هذا تنفيذًا ولا تعديل RFC (§26.2 في ADR-0045).
- **ما لم يعتمده المالك**: **A44-01** (بنية Trust-First V2 لـ`.unit`) — لم يصدر اعتماد مالكي صريح؛ تبقى **OWNER DECISION REQUIRED** إلى جانب A44-07/08/09/10 وبنود B8 الفرعية A45-03/04/05/08 (النطيق) /09/10. لا يُحوَّل أي حكم مراجعة سابق إلى اعتماد.
- **مفتاح Root**: كان **BLOCKED — EXTERNAL PREREQUISITE** (A44-06) — **أُلغي لاحقًا**: اعتماد مالكي + مراسم منفَّذة موثّقة في `docs/security/A44-06-production-root-key-certification.md` (2026-08-15).
- **حالة RFC-to-ADR بعد الاعتماد**: RFC §3.10 و§3.12 D2 **لم يُعدَّلا**؛ ADR-0008 **لم تُحَل**؛ mirror ADR-0038 **لم يُحدَّث**؛ وثيقة التجميد **لم تتغير** — تعديل RFC يبقى خطوة حوكمة منفصلة لاحقة (Freeze §4 Step 3) حتى بعد اعتماد القرارات.
- **حالة التنفيذ**: **NO IMPLEMENTATION AUTHORIZED** — الاعتماد المالكي لا يساوي تفويض تنفيذ؛ يلزم تنفيذ بوابة تفويض تنفيذ منفصلة.
- **حدود البوابة**: وثائقية فقط (ADR-0044، ADR-0045)؛ فهرس ADRs دون تغيير (الحالة PROPOSED قائمة لأن Step 3 لم يُنفَّذ)؛ لا رمز، لا مفاتيح، لا RFC، لا وثيقة تجميد.

## 28.5 نتيجة بوابة استكمال قرارات المالك (Owner Decision Completion Gate — 2026-08-14)

- **الغرض**: إعادة فحص جميع القرارات المتبقية في ADR-0044 وADR-0045 وفق قاعدة البوابة: **لا يصبح أي قرار OWNER RATIFIED إلا بنص تعليمات مالكية صريح في تعليمات البوابة نفسها**.
- **النتيجة**: البوابة **لم تورد أي قرار مالكي جديد صريح** — مصفوفة [ADR-0045 §26](docs/architecture/0045-b8-first-identity-access-import.md) دون تغيير: A45-01/A45-02 تبقى OWNER RATIFIED (تأكيد §26.5)؛ **A44-01** وجميع البنود المعلقة (A44-07/08/09/10 ونصفا السياسة لـA44-11/12، وبنود B8 الفرعية A45-03/04/05/08-النطيق/09/10) تبقى **OWNER DECISION REQUIRED**؛ **A44-06** تبقى **BLOCKED — EXTERNAL PREREQUISITE**.
- **لا تحويل**: الملاءمة الفنية وحكم «APPROVED WITH BLOCKERS» والتوصيات **لا تتحول إلى اعتماد مالكي** بأي حال.
- **الحدود**: لا تنفيذ، لا تعديل RFC، لا تغيير لوثيقة التجميد، لا تغيير لـADR-0038/ADR-0008، لا تغيير للفهرس (الحالة PROPOSED قائمة)، لا أي تفويض تنفيذي.

> **سجّل تاريخي محفوظ (2026-08-14)**: حالتُه تُجاوزت ببوابة اكتمال الاعتماد اللاحقة — كل البنود المذكورة أعلاه أقرَّها المالك لاحقًا (سجل §26.6 في ADR-0045).

## 28.6 OWNER RATIFICATION RECORD — COMPLETED (2026-08-14)

**سجل اكتمال اعتماد المالك** — المصدر: قرارات المالك الصريحة في تعليمات بوابة اكتمال الاعتماد (الأقسام 1–18). السجل الكامل (المصفوفة الموحدة لكل قرار: المعرّف، القرار، الحالة، الحدود، التبعيات) في **[ADR-0045 §26.6](docs/architecture/0045-b8-first-identity-access-import.md)** — لا يُكرَّر الجدول هنا.

- **OWNER RATIFIED (17)**: A44-01 (بنية Trust-First V2/Ed25519 — 15 نقطة معمارية، اعتماد معماري فقط) · A44-07 (إغلاق نافذة V1 بمعايير أدلة، بلا تاريخ تقويمي) · A44-08 (أول `package_sequence` = 1) · A44-09 (استراتيجية fleet-sync) · A44-10 (تدوير Root بنافذة قبول صريحة، بلا استبدال صامت) · A44-11 (نموذج الربط الحالي: unit_code + مرساة WILAYA + مراسم التوفير؛ ربط الجهاز مؤجل) · A44-12 (التراجع = إعادة توفير؛ بلا تراجع V2→V1) · A45-01 (خيار A) · A45-02 (الضوابط الخمسة عشر الإلزامية) · A45-03 (مجهول على طبقة التخويل؛ بلا حساب Admin مؤقت) · A45-04 (قابلية تدقيق الانتقال: تركيب المرساة + أول استيراد + النتائج) · A45-05 (دلالات تدقيق مخصصة؛ المعرّفات تفصيل تنفيذي) · A45-08 (دور `.unit` User-only؛ `role=Admin` غير صالح) · A45-09 (سياسة الاسترداد الستّية؛ تقبّل تبديل UUID) · A45-10 (تقبّل خطر النسخ؛ ربط الجهاز مؤجل) · A45-11 (المبدأ فقط: A44-06 محجوب ← نشر B8 محجوب) · A45-12 (SEC-002 منفصلة؛ لا `issue_first_admin_key` على UNIT).
- **BLOCKED — EXTERNAL PREREQUISITE (1)**: A44-06 (اعتماد مفتاح Root الإنتاجي — يبقى ناقل TEST-2 دون مساس؛ لا توليد، لا شهادة، لا ادعاء بمراسم) — **أُلغي لاحقًا (2026-08-15): A44-06 CERTIFIED** — راجع `docs/security/A44-06-production-root-key-certification.md`.
- **OUT OF SCOPE (1)**: A44-13 (توزيع App Key — عملية مستقلة).
- **حالة قرارات A45-06/A45-07**: DECIDED — FACT (مقيدة بالكود/الدليل) — دون تغيير.
- **حدود شاملة لكل الصفوف**: تفويض التنفيذ = **NO** · تفويض تعديل RFC = **NO** · تعديل وثيقة التجميد = **NO** · تعديل ADR-0038/ADR-0008 = **NO** · حالة ADR = **Proposed** (لم يُنفَّذ Step 3 من Freeze §4) · لا توليد/تعديل لأي مادة مفتاحية.
- **الخطوة التالية**: بوابة حوكمة تعديل RFC منفصلة (Step 3: ربط RFC §3.10/§3.12 D2 وADR-0038 وإحالة ADR-0008 وتحديث وثيقة التجميد)، ثم لاحقًا بوابة تفويض تنفيذ مستقلة. لا شيء من هذا مُصرَّح به في هذه البوابة.

> **تحديث لاحق (2026-08-14 — RFC-AMENDMENT GOVERNANCE GATE):** بند «الخطوة التالية»
> أعلاه نُفِّذ جزئيًا — تعديل RFC §3.10/§3.12 D2 ومرآة ADR-0038 §6 تمّا وثائقيًا
> (§28.7). المتبقي من Step 3: حالة Accepted + تحديث وثيقة التجميد (بوابة وثائقية
> لاحقة)، وإحالة ADR-0008 (مؤجلة حتى إغلاق نافذة V1 بالأدلة).

## 28.7 نتيجة بوابة تعديل RFC (RFC-Amendment Governance Gate — 2026-08-14)

- **السلطة الحوكمية**: Freeze §4 Step 3 — «Once the RFC is approved»: اعتماد المالك (ADR-0044/0045، اكتمل 2026-08-14) = موافقة الـ RFC؛ تُنفَّذ التعديلات الوثائقية الدنيا لتعكس البنية المعتمدة. Step 4 (تنفيذ) غير مفعَّل.
- **ما نُفِّذ (وثائقيًا فقط)**:
  - RFC `2026-08-04-node-identity-trust.md` §3.10: استبدال «استثناء Bootstrap الدائم» لـ `.unit` بنص Trust-First V2/Ed25519 المعياري (WILAYA سلطة، مرساة ACTIVE مثبَّتة قبل القبول، User-only، أول تسلسل = 1، لا استبدال صامت، فشل مغلق، تراجع = إعادة توفير، دوران جذر بنافذة قبول، HMAC-V1 إرث قيد التقاعد بنافذة أدلة A44-07 وانتقال fleet-sync A44-09) + حفظ السياق التاريخي.
  - RFC §3.12: صف «الترتيب المعياري للثقة» (مرساة قبل `.unit` V2 وقبل أول `identity_access`) + صف «أول استيراد `identity_access` على عقدة UNIT جديدة (B8 bootstrap)» في جدول B6-A بنص ADR-0045 A45-01..05/12 المعياري.
  - ADR-0038 §6: مرآة نفس التعديل (بعلامة «تعديل مرآة 2026-08-14»).
- **ما لم يُنفَّذ**: تعديل `ARCHITECTURE_FREEZE.md` (محظور في هذه البوابة — يبقى لبوابة Step 3 النهائية) · إحالة ADR-0008 (مؤجلة — معايير A44-07 غير متحققة؛ الـ RFC يُبقي HMAC-V1 قراءة إرثية) · تغيير حالة ADR-0044 (تبقى **Proposed**) · أي رمز/اختبار/إعدادات.
- **أثر عائق Root (A44-06)**: لا يمنع تعديل RFC (وثيقة قرار) ولا تعديل ADR — يمنع **جاهزية الإنتاج فقط**؛ النص المعدَّل يذكر الجذر «الإنتاجي» كشرط تشغيلي، ولا يُدّعى أي اعتماد.
- **الحدود**: تعديل RFC ≠ تفويض تنفيذ؛ تعديل RFC ≠ اعتماد إنتاج؛ لم يُولَّد أي مفتاح؛ لا شيء مكدّس أو ملتزم.

## 28.8 نتيجة بوابة حالة ADR النهائية (Final Freeze/ADR Status Governance Gate — 2026-08-14)

- **إغلاق Freeze §4 Step 3 (Step 3.1–3.4)**: الـ RFC موافق عليه (قبول + اعتماد مالكي للبنية)؛ الوثيقة تنتقل من **Proposed** إلى **Accepted** (Step 3.2)؛ الربط بالـ RFC قائم (Step 3.3)؛ وثيقة التجميد حُدِّثت بأثر القرار (Step 3.4 — §2.2 بند B8 في ADR-0045، §2.7 بند `.unit` Trust-First، §2.8 بند عائق Root).
- **التمييز المحفوظ**: القبول المعماري ≠ تفويض تنفيذ ≠ جاهزية إنتاجية. التنفيذ **NOT AUTHORIZED** (باب مستقلة قادمة)؛ الإنتاج كان **BLOCKED** بعائق A44-06 — **أُلغي لاحقًا (2026-08-15): اعتماد المفتاح اكتمل** (سجل: `docs/security/A44-06-production-root-key-certification.md`)؛ جاهزية الإنتاج تُفحص في بوابة مستقلة لاحقة.
- **إحالة ADR-0008**: مؤجلة (معايير إغلاق نافذة V1 المبنية على الأدلة غير متحققة — A44-07)؛ الوثيقة لم تُمسَّ.
- **السجل التاريخي**: تُحفظ جميع بوابات الحوكمة السابقة (28.1–28.7) كما هي.
- **الحدود**: وثائقية فقط؛ لا رمز، لا اختبارات، لا ترحيلات، لا مفاتيح؛ لا شيء مكدّس أو ملتزم.

**لا يُنفَّذ أي تغيير تنفيذي قبل موافقة المالك واستكمال عملية RFC-to-ADR.**