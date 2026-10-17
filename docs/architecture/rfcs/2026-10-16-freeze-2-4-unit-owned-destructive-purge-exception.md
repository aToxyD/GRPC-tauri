# RFC: Narrow `ARCHITECTURE_FREEZE.md` §2.4 Exception for the ADR-0064 UNIT-Owned Destructive Purge (استثناء ضيّق في بند §2.4 لتجميد البنية لأغراض الحذف المملوك للوحدة وفق ADR-0064)

**تاريخ:** 2026-10-16
**الحالة:** 🟢 **Accepted (2026-10-17)** — اعتمد المالك هذا الـ RFC رسميًا بموجب القرار Q1 (الخيار A)، وطُبِّق تعديل §2.4 في `ARCHITECTURE_FREEZE.md`، وحُسمت القرارات Q1–Q6، وأُذن بتنفيذ ADR-0064 البرمجي.
**النطاق:** بند مجمّد واحد فقط — `docs/architecture/ARCHITECTURE_FREEZE.md` §2.4، السطر 126 (*"🔒 Immutable closed years"*). لا يتناول هذا المقترح أي بند مجمّد آخر.
**المالك:** Architecture / Security / Data Governance
**المراجع:** ADR-0064 → `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md`؛ العملية الرسمية → `docs/architecture/ARCHITECTURE_FREEZE.md` §4 (الأسطر 224–272)؛ عقد الحوكمة → `AGENTS.md` §8 C1 (السطر 310) و§12 (الأسطر 397–404)؛ الميثاق → `docs/architecture/ARCHITECTURAL_INVARIANTS.md` §3 (الأسطر 57–72)؛ فهرس القرارات → `docs/architecture/ADR_INDEX.md` (الفهرس، السطر 92؛ السياسة، الأسطر 112–118)؛ سابقة مطبَّقة مماثلة → `docs/architecture/rfcs/2026-08-09-backup-async-exception.md` + ADR-0043.

> **إعلان حوكمي (Governance declaration).** هذا المستند هو **وثيقة التعديل المعتمدة (Accepted RFC)** المنصوص عليها في `ARCHITECTURE_FREEZE.md` §4 لتعديل بند مجمّد في القسم 2 (`AGENTS.md` §8 C1). اكتملت مراجعة الخطوة 2 والاعتماد في الخطوة 3 بقرار مالك المشروع (القرار Q1 — الخيار A)، وطُبّق التعديل في `ARCHITECTURE_FREEZE.md` §2.4، مع استيفاء كافة فحوصات الحوكمة والاختبارات الشاملة بنجاح.
>
> **حسم القرارات والاعتماد.** حسم المالك الأسئلة **Q1–Q6**: **Q1** الخيار A (تعديل `ARCHITECTURE_FREEZE.md` §2.4) و**طُبِّق** التعديل المطابق للمسودة §3.1 عبر عملية RFC-to-ADR؛ **Q2/Q3** الخيار B (تأجيل إلى مراجعة مخطط منفصلة)؛ **Q4/Q5** الخيار A (سياسة معتمدة غير مُنفَّذة)؛ **Q6** الخيار A (مُصدَّق). تفاصيل قرارات Q2–Q5 مُسجَّلة في `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` §9.3.3.

---

## 1. Problem Statement (بيان المشكلة)

### 1.1 الوضع القائم والفجوة الحوكمية

تنص وثيقة التجميد على أن:

* بند `ARCHITECTURE_FREEZE.md` §2.4 في القسم المجمّد (السطر 126): **"🔒 Immutable closed years — closed fiscal years receive no new mutations. Cache entries for closed years are never invalidated."**
* السطر 95: البنود المجمّدة *"may not be modified without an approved ADR"*.
* السطر 226: *"Any change to a **frozen** item (Section 2) requires the following process:"* ثم الخطوات 1–5.

في المقابل، يقرر ADR-0064 (المقبول 2026-10-05) **استثناءً حياة-دورية تدميريًا واحدًا وضيّقًا**: أثناء تنفيذ حذف وحدة (UNIT) عبر معاملة الحذف الخاصة بـ ADR-0064، يجوز إتلاف الصفوف **المملوكة للوحدة** حتى وإن كان `fiscal_year` / `origin_fiscal_year` / `report_year` الخاص بها **مؤرشفًا** (ADR-0064 §6.1، والقرار الرسمي **D15**). ويوثّق ADR-0064 §7.3 أن بند §2.4 **"Qualified"** أي **معاد-النطاق** لا مُلغى: فقرة *"Immutable closed years"* وفقرة *"Cache entries ... never invalidated"* **لا تمنعان** حذف الصفوف المملوكة للوحدة ضمن عملية الإتلاف تلك، بينما تظلّان تمنعان كل تعديل آخر على سنة مغلقة.

الفجوة: ADR-0064 **يؤهّل** §2.4 نصًّا (qualifies) لكنه **لا يعدّله** (لا يُحرّر وثيقة التجميد). وقد سجّل ADR-0064 هذا صريحًا في §9.3 **Q1**:

> *"Does this ADR's `ARCHITECTURE_FREEZE.md` §2.4 re-scoping require a formal freeze-document amendment (RFC-to-ADR Step 3.4), or is this ADR's qualification sufficient?"* — النتيجة الموثّقة: *"If required, the freeze document must be amended through the RFC-to-ADR process before implementation; the ADR remains the qualification record either way."* (`0064:1574`)

**تحديث — حُسم Q1:** حسم المالك لاحقًا **Q1 بالخيار A** (التعديل مطلوب)، و**طُبِّق تعديل §2.4** المطابق للمسودة §3.1 عبر عملية RFC-to-ADR. يبقى ADR-0064 هو **سجل التأهيل**، ولا يحرّر وثيقة التجميد بنفسه.

### 1.2 مبرر التغيير

يقضي عقد الحوكمة في المستودع بأن أي تغيير على بند في القسم المجمّد §2 يستلزم عملية RFC-to-ADR كاملة (`AGENTS.md` §8 C1، السطر 310؛ `ARCHITECTURE_FREEZE.md` السطر 226)، وأن الخطوة 3.4 تفرض **تحديث وثيقة التجميد** لتعكس التغيير (`ARCHITECTURE_FREEZE.md:256`)، ولا يبدأ التنفيذ البرمجي قبل اكتمال الاعتماد (`:258`؛ `ARCHITECTURAL_INVARIANTS.md:69`). وبما أن ADR-0064 §7.3 **يؤهّل** بند §2.4 دون أن يحرّره، فالمسار المتاح لتعديل وثيقة التجميد هو تقديم هذا الـRFC بصياغة ضيّقة لا تتجاوز القرار الرسمي المقبول.

> **حالة Q1 (محسومة لاحقًا بقرار المالك — الخيار A).** كان ADR-0064 §9.3 Q1 يسجّل أن ما إذا كان تعديل وثيقة التجميد **مطلوبًا** أصلًا هو **سؤال حوكمي** *"a governance-process decision, not derivable from the code"* (`0064:1574`). **حسمه المالك لاحقًا بالخيار A** (التعديل مطلوب)، و**طُبِّق تعديل §2.4** المطابق للمسودة §3.1 عبر عملية RFC-to-ADR. هذا الـRFC نفسه لم يحسم Q1 ولا استبق قرار المالك؛ القرار صدر عن المالك (راجع §3.4).

### 1.3 ما لا يعالجه هذا المقترح

هذا الـRFC **لا** يحلّ ولا يوحي بحلّ الأسئلة **Q2** (مفتاح خارجي لـ `daily_report_meal_items.fifo_layer_id`)، **Q3** (تشديد `NOT NULL`)، **Q4** (فرض `allocation.unit_id == contract.unit_id` على مسار الاستيراد في الولاية)، **Q5** (سؤال وجود مسار إصلاح معتمد عبر الوحدات — دون افتراض وجوده). كما **لا** يعيد فتح قرار **Q6** (المُصدَّق: الخيار A — الرفض المغلق-بالفشل). **ملاحظة:** حسم المالك Q2–Q5 لاحقًا و**منفصلةً عن هذا الـRFC** (Q2/Q3: تأجيل إلى مراجعة مخطط منفصلة؛ Q4/Q5: سياسة معتمدة غير مُنفَّذة)، ويسجّلها ADR-0064 §9.3.3. راجع §4.4 و`Appendix C`.

---

## 2. Current Behavior (السلوك الحالي — العقد المجمّد)

### 2.1 بند §2.4 القائم (نصًّا)

`ARCHITECTURE_FREEZE.md` §2.4 "Fiscal Semantics" يضمّ البنود المجمّدة التالية (الأسطر 122–129)، وأهمّها لهذا المقترح:

| # | البند المجمّد (السطر) | النص |
|---|---|---|
| A | **Immutable closed years** (`:126`) | *"closed fiscal years receive no new mutations. Cache entries for closed years are never invalidated."* |
| B | **Origin immutability** (`:128`) | *"`fifo_stock_layers.origin_fiscal_year` is immutable after creation. `OriginImmutable` invariant."* |
| C | **FIFO reclassification, not deletion** (`:125`) | *"year close reclassifies `source_type` from `ORDER` to `OPENING`. No data moved, copied, or deleted."* |
| D | **Atomic year closure** (`:124`)، **Single open year** (`:127`)، **All-or-nothing transitions** (`:129`) | لا تتأثّر بهذا المقترح. |

### 2.2 ما يقرّره ADR-0064 اليوم (مُصدَّق)

* **§6.1 (القرار D15):** استثناء تدميري صريح يقتصر على **الحذف المملوك للوحدة**: صفوف المجموعة المملوكة للوحدة وفق §6.1 (Class-A) قابلة للإتلاف حتى عندما تكون سنتها المالية مؤرشفة. وبيان صريح في `0064:1165–1170` بأنه **"an explicit destructive lifecycle exception to `ARCHITECTURE_FREEZE.md` §2.4's 'Immutable closed years' bullet"**، محصور في *"UNIT-owned destructive purge and nothing else"*.
* **§6.2 (القرار D21):** مجموعة محمية **نهائية** (node-global / مشتركة) تبقى دون مساس. **المصدر المرجعي الحصري للتعداد هو ADR-0064 §6.2 نفسه**، ويضمّ كذلك `audit_log`/`audit_summary`، وسجلات B13–B15، وحارس تراجع الاستعادة. الأمثلة التالية **غير حصرية**: `fiscal_year_status`, `opening_balance_snapshots`, `fiscal_export_snapshots`, `fiscal_operational_snapshots`, `applied_fiscal_transitions`, `fiscal_closure_package_registry`, `fiscal_year_tax_policy`, `inventory_stocks`, `products`, `suppliers`. **لا إعادة اشتقاق، ولا إعادة نطاق، ولا يضيّق هذا الـRFC التعداد النهائي.**
* **§7.3:** قائمة دقيقة لما *ينجو دون تغيير* (السنة المالية الصريحة كعمود، الإغلاق الذرّي، إعادة تصنيف FIFO، السنة المفتوحة الواحدة، الانتقالات all-or-nothing) وما *يُؤهَّل* (الفقرة الأولى من Immutable closed years فقط). **ملاحظة:** يُسجِّل §9.1 C-c وصفًا لدلالة فقرة cache invalidation (انظر §3.3 أدناه). **تحديث:** حسم المالك لاحقًا (Q1) إبقاء فقرة «Cache entries … never invalidated» **نافذةً دون إعادة نطاق**، بما يوافق §9.1 C-c؛ راجع §3.3 و§3.4.
* **§4.6/§4.7 (Preflight):** فحص أولي مغلق-بالفشل **قبل أي `DELETE`**، بما فيه قاعدة **Q6** المُصدَّقة (الخيار A) — وهي **قاعدة نافذة قائمة** في ADR-0064 §4.7 D4 و§9.3.3، ولا يكرّرها هذا المقترح ولا يغيّرها.
* **§9.3 Q1 (محسوم لاحقًا):** ما إذا كان تعديل وثيقة التجميد مطلوبًا — **حسمه المالك بالخيار A** وتقرّر التعديل وطُبِّق.

### 2.3 حالة وثيقة التجميد (وقت كتابة الـRFC، ثم التحديث)

وقت كتابة هذا الـRFC كانت وثيقة `ARCHITECTURE_FREEZE.md` **غير محرَّرة** بخصوص §2.4 ولم يكن يقع تحتها بند استثناء. **تحديث:** حُسم Q1 لاحقًا بالخيار A، و**أُضيف بند الاستثناء الفرعي تحت §2.4** المطابق للمسودة §3.1 عبر عملية RFC-to-ADR؛ وتنص الفقرة الختامية (السطر 335) على: *"It is updated only through the RFC-to-ADR process."*

---

## 3. Proposed Change (التغيير المقترح — غير معتمد)

### 3.1 التعديل المقترح على §2.4

يُقترح إدراج سطر فرعي واحد **مباشرة بعد** `ARCHITECTURE_FREEZE.md` السطر 126، على نمط النمط المطبَّق فعلًا في §2.6 بخصوص ADR-0043 (الأسطر 145–149):

```markdown
- **🔒 Immutable closed years** — closed fiscal years receive no new mutations. Cache entries for closed years are never invalidated.
  **Sanctioned exception (ADR-0064 — Accepted 2026-10-05) — limited to the first sentence of this
  bullet ("closed fiscal years receive no new mutations"):** that sentence does not bar the
  UNIT-owned destructive purge defined by `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md`
  §6.1 and executed by the ADR-0064 deletion transaction; the §6.1 Class-A UNIT-owned rows MAY be
  purged even when their `fiscal_year` / `origin_fiscal_year` / `report_year` is archived. This is a
  **permanent business-policy** carve-out, not a temporary `[arch:allow-*]` code suppression, and it
  is not subject to the 90-day expiry rule for temporary architecture suppressions
  (`docs/architecture/ARCHITECTURAL_INVARIANTS.md` §3). The
  exception is scoped to the UNIT-owned destructive purge and nothing else: it does not authorize
  any other mutation of a closed year and does not change what "closed" means for any other write
  path. This exception does **not** modify, address, or authorize the second sentence of this
  bullet — "Cache entries for closed years are never invalidated" — and it does **not** authorize
  the invalidation of cache entries for closed fiscal years. §2.4's other bullets, §2.3 in its
  entirety, and §2.5 determinism remain in force. The §6.2 protected set remains fully protected
  and is never re-derived or re-scoped. The deletion transaction's fail-closed preflight and
  ownership-integrity rules are governed by ADR-0064 §4.6–§4.7 and §9.3.3, not restated here.
```

> ملاحظة صياغة: النص أعلاه **يُشير** إلى قاعدة Q6 (عبر §4.6–§4.7 و§9.3.3) ولا **يعيدها**، تفاديًا لنسخ قاعدة معيارية في موضعين ومخالفةً لمبدأ المصدر الواحد (P2/D2 في `AGENTS.md` §1 و§7).

### 3.2 نطاق التعديل (Scope & Boundaries)

* **يدخل في النطاق:** بند `ARCHITECTURE_FREEZE.md` §2.4، السطر 126 فقط.
* **لا يدخل في النطاق:** كل بند آخر في §2.2، §2.3، §2.4، §2.5–§2.8؛ ومسودة أي قاعدة Q6 معيارية؛ وأي تغيير على ADR-0064 أو `ADR_INDEX.md` **داخل هذا الـRFC**.
* **طبيعة التغيير:** **توثيقي بحت** (نص وثيقة التجميد). لا يغيّر أي سلوك تشغيلي، ولا أي مخطط، ولا أي كود.

### 3.3 غموض مسجَّل — دلالات إبطال الذاكرة المؤقتة (cache invalidation) — حسمه المالك لاحقًا (Q1)

سُجِّل في ADR-0064 نفسه توتّر صريح بين وصفين لدلالة فقرة إبطال الذاكرة المؤقتة:

* **§7.3** يُدرِج الفقرة *"Cache entries for closed years are never invalidated"* تحت البند **"Qualified"**، ويقول إنها هي وفقرة *"Immutable closed years"* *"do not bar the §6.1 UNIT-owned purge"*، وإنهما *"continue to bar every other mutation of a closed year."*
* **§9.1 C-c** يعتمد على أن `ARCHITECTURE_FREEZE.md` §2.4 *"forbids invalidating closed-year cache entries"* كحقيقة قائمة (ليصف `fiscal_export_snapshots` بأنه non-invalidatable وتبقى `movement_count`/`report_count` تعكس حالة ما قبل الحذف *"forever"*)، ويصرّح بأن ذلك *"a direct, unavoidable consequence ... It is stated, not resolved."*

هذان الوصفان **غير متطابقين بشكل قاطع**؛ لذلك، ووفق التعليمات، **لا يحسم هذا المقترح** التفسير. بدلًا من ذلك:

* مقترح §3.1 **يُبقي** الفقرة الثانية *"Cache entries for closed years are never invalidated"* **دون أي تعديل أو إعادة نطاق**.
* مقترح §3.1 **لا يمنح** أي تخويل لإبطال أي مفتاح ذاكرة مؤقتة لسنة مالية مغلقة.
* إن كان التفسير الصحيح يقتضي أن يتناول التعديل الفقرة الثانية أيضًا (كما قد يوحي §7.3)، فهذه **مسألة كانت مفتوحة تُترك لمراجعة الخطوة 2 وقرار المالك**، وليست من اختصاص صائغ هذا الـRFC.

**تحديث (قرار المالك Q1).** أبقى المالك الفقرة الثانية **دون إعادة نطاق**، ويُصرّح تعديل §2.4 المنفَّذ بأنه **لا يخوّل** إبطال مفاتيح الذاكرة المؤقتة لسنة مالية مغلقة، وبأنّ `fiscal_export_snapshots` للسنة المغلقة **لا تُبطَل ولا تُعدَّل ولا تُعاد اشتقاقًا** بسبب حذف وحدة، مع الاحتفاظ بالقيم الملتقَطة وقت الإصدار الأصلي (موافقًا §9.1 C-c).

### 3.4 قرار المالك — الاستثناء **سياسة عمل دائمة** (لا تصريح `[arch:allow-*]` مؤقت)

**قرار المالك (2026-10-16، مُسجَّل بموجب مهمة الحوكمة هذه):** استثناء الحذف المملوك للوحدة للسنة المالية المؤرشفة وفق ADR-0064 هو **سياسة عمل دائمة (permanent business policy)** — لا تصريح كود مؤقت من نوع `[arch:allow-*]`. وبالتالي **لا يخضع** لقاعدة انتهاء الصلاحية (90 يومًا) المنصوص عليها في `ARCHITECTURAL_INVARIANTS.md` §3 (السطران 71–72) للتصريحات المؤقتة، بل يُصنَّف على نمط carve-out وثيقة التجميد الثابت المطبَّق في §2.6 (سابقة ADR-0043).

* **تمييز صريح عن سجل التصريحات المؤقتة:** التصريحات الـ**26 النشطة** المسجَّلة في `docs/architecture/adr_exception_registry.md` تخصّ مجالات أخرى (المزامنة، النسخ الاحتياطي/الاستعادة، مؤشرات التقدّم في الواجهة) **ولا علاقة لها بسياسة حذف الوحدات**؛ وهي تبقى خاضعة لدورة التجديد. أما استثناء §2.4 هذا فلا يُسجَّل كتصريح `[arch:allow-*]`، ولا يُضاف إلى ذلك السجل.
* **قيود تنفيذ ملزمة (عند الخطوة 4، لا عند هذا الـRFC):** يجب ألّا يُدخِل تنفيذ حذف الوحدة أي تصريح `[arch:allow-*]` جديد — تبقى استعلامات SQL في `src-tauri/src/repositories/` ولا تُوضع في `UnitService`؛ ولا تُضاف خيوط خلفية (`std::thread::spawn`) ولا أعمال غير متزامنة إلى مسار الحذف؛ ولا تُستخدم أنماط احتياط أعمى مثل `.unwrap_or()` لطمس الأخطاء؛ وتُنشَر الأخطاء عبر نموذج أخطاء التطبيق القائم بسلوك **مغلق-بالفشل**. (يُوثَّق هذا القيد للتنفيذ المستقبلي فقط؛ لا يعدّل هذا الـRFC أي أداة أو أي سجل تصريحات.)
* **حدٌّ صريح — ما يحسمه القرار وما لا يحسمه:** قرار المالك يحسم **ديمومة هذه السياسة فقط**. وهو **ليس** اعتمادًا من مراجعي الخطوة 2، و**لا يعتمد** هذا الـRFC، و**لا يؤذن** بالخطوة 3 أو بأي تنفيذ، و**لا يعفي** أي متطلب حوكمي آخر في `AGENTS.md` §8/§12 أو في وثيقة التجميد. **حُسمت Q1 لاحقًا بالخيار A** (تعديل §2.4، وقد طُبِّق)؛ وتبقى متطلبات اعتماد هذا المستند كوثيقة (مراجعة الخطوة 2 والاعتماد في الخطوة 3) قائمة كما هي.

---

## 4. Impact Analysis (تحليل الأثر — بنود Section 2 المتأثرة)

### 4.1 البند المجمّد المتأثّر

| البند المجمّد | الأثر |
|---|---|
| `ARCHITECTURE_FREEZE.md` §2.4 "Immutable closed years" (السطر 126) | **يُضاف إليه سطر فرعي استثنائي ضيّق** (المسودة في §3.1). يبقى الأصل قائمًا؛ الاستثناء نطاقه محصور في §6.1. |

### 4.2 البنود المجمّدة الأخرى، وحالة "Origin immutability"

| البند | الحالة بعد التعديل المقترح |
|---|---|
| §2.4 "FIFO reclassification, not deletion" (`:125`) | **دون تغيير** — إغلاق السنة لا يزال لا ينقل/ينسخ/يحذف شيئًا. |
| §2.4 "Atomic year closure" / "Single open year" / "All-or-nothing transitions" (`:124`,`:127`,`:129`) | **دون تغيير**. |
| **§2.3 Audit Trail** بالكامل (`:113–120`) | **دون تغيير** — يبقى dual-write، hash-chain، transactional-audit، additive-only، single-repository. |
| **§2.5 Reproducibility & Determinism** بالكامل (`:131–140`) | **دون تغيير** — لا نداء ساعة حائط، لا عشوائية، ترتيب حتمي، قابلية إعادة إنتاج بايت-ببايت. |

**§2.4 "Origin immutability" (`:128`) — غير مُدرَج في تعديل §3.1، مع تِبايُن مصادر غير محسوم:**
لم يتضمّن §3.1 أي تعديل على بند *"Origin immutability"*. غير أن **مصادر ADR-0064 لا تتّفق** في وصف هذا البند:

* **ADR-0064 §1 (`0064:66`)** يصنّفه صراحةً ضمن ما **"'Re-scoped, not edited'"** — أي أن §6 يعيد نطاق بندَي *"Immutable closed years"* **و** *"Origin immutability"* معًا، محصورًا في الإتلاف المملوك للوحدة.
* **ADR-0064 §6.3 (`0064:1220–1224`)** يفسّره بأنه قاعدة تخصّ **قيمة الصف عبر الزمن**، وأن إتلاف الصف *"is outside that rule's scope"*، وأن أي اختبار §2.4 قائم لـ"Origin immutability" كان يفترض عدم حذف صفوف `fifo_stock_layers` *"would have to be re-scoped by the implementation"*.

**لا يحسم هذا الـRFC** ما إذا كان بند "Origin immutability" يحتاج هو أيضًا إلى تعديل، **ولا يوسّع نطاق §3.1** ليشمله من تلقاء نفسه؛ فالمصادر لا تحسم ذلك بشكل قاطع، والمسألة **تُترك كبند تقريري لمراجعة الخطوة 2 وقرار المالك/المراجعين المعيَّنين**. الافتراض العملي في هذا الـRFC هو الإبقاء على نطاق §3.1 كما هو (فقرة *"Immutable closed years"* فقط) حتى قرار صريح بخلاف ذلك.

### 4.3 الحدود المحمية (ADR-0064 §6.2)

المجموعة المحمية النهائية (D21) تبقى **محمية بالكامل**. **التعداد المرجعي والحصري هو المُعدَّد في ADR-0064 §6.2** (ويضمّ أيضًا `audit_log`/`audit_summary`، وسجلات B13–B15، وحارس تراجع الاستعادة)؛ والأمثلة التالية **غير حصرية**: `fiscal_year_status`, `opening_balance_snapshots` (node-global، لا تُعاد اشتقاقًا)، `fiscal_export_snapshots`, `fiscal_operational_snapshots`, `applied_fiscal_transitions`, `fiscal_closure_package_registry`, `fiscal_year_tax_policy`, `inventory_stocks` (لا بعد وحدة — تُحفظ بلا تعديل)، `products`, `suppliers`. النص المقترح في §3.1 يُحيل إلى مجموعة §6.2 **كاملة** دون إعادة تعدادها، ويصرّح بأنها تبقى محمية بالكامل دون إعادة اشتقاق أو إعادة نطاق.

### 4.4 خارج النطاق صراحةً (Q2–Q5 وQ6)

هذا الـRFC **لا يحلّ** الأسئلة **Q2** أو **Q3** أو **Q4** أو **Q5**، و**لا يتضمّن أي قرار ضمني** بشأنها (وقد حسمها المالك لاحقًا و**منفصلةً عن هذا الـRFC**؛ انظر §1.3 و§9.3.3 في ADR-0064). نطاق المقترح محصور في الاستثناء الضيّق لبند §2.4 (المادة §3.1)؛ وأي خيار تصميمي أو مخطّطي أو مسار كتابة أو ضمان يخصّ تلك الأسئلة يبقى **خارج هذا المقترح بالكامل**، وتُتَّخذ القرارات المتعلقة به عبر مساراتها الحوكمية المستقلة، لا عبر هذا الـRFC.

* **Q2 / Q3 / Q4:** لا يقرّر هذا المقترح شيئًا — لا بقبول ولا برفض ولا بتأجيل — بخصوص إضافة مفتاح خارجي، أو تشديد `NOT NULL`، أو أي فرض على مسار الاستيراد؛ كما **لا يصنّف أي فحص قائم (preflight) كضمان نهائي أو وحيد** لتلك الأسئلة. (للحالة الحالية والأثر غير المحسوم لكل سؤال راجع ADR-0064 §9.3.)
* **Q5:** لا يُعرَّف في هذا المقترح أي **إجراء إصلاح معتمد**، ولا يُوصى به، ولا يُخوَّل، ولا يُفترض وجوده. المذكور في ADR-0064 هو أن عملية الحذف مصمَّمة للفشل المغلق (fail-closed) عند اكتشاف تعارض؛ وهذا المقترح **لا يضيف** أي آلية إصلاح ولا يعتمد أي مسار من هذا القبيل.
* **Q6:** يبقى كما صُدِّق (الخيار A — الرفض المغلق-بالفشل). هذا المقترح **يشير** إليه (عبر §4.6–§4.7 و§9.3.3) ولا يكرّره ولا يعدّله ولا يضعفه ولا يعيد فتحه.

**لا أثر على الحتمية/Single-writer/WAL أو أي بيانات؛ التعديل وثائقي فقط.**

---

## 5. Migration Plan (خطة الترحيل — تسلسل حوكمي، لا ترحيل بيانات)

1. **الخطوة 1 (هذا المستند):** كتابة الـRFC — *مكتمل بالمسودة الحالية*.
2. **الخطوة 2 — Review:** مراجعة الـRFC من **Lead Architect** (السلامة المعمارية)، و**Security Officer** (الأثر الأمني)، و**Domain expert** (الصحّة المالية/المحاسبية) — وفق `ARCHITECTURE_FREEZE.md:238–249`.
3. **الخطوة 3 — Approve & Create/Update ADR:** بعد اعتماد الـRFC: *(حالة: طُبِّق تعديل §2.4 — البند 2 أدناه — بموجب قرار المالك Q1 بالخيار A؛ أما اعتماد هذا الـRFC وربطه الرسمي فما زالا معلَّقَين.)*
   1. **ربط الـRFC** في سجل ADR-0064 (حقل `# Reference`) — `ARCHITECTURE_FREEZE.md:255`.
   2. **تعديل وثيقة التجميد §2.4** بالمسودة في §3.1 — `ARCHITECTURE_FREEZE.md:256` (الخطوة 3.4). **حالة: طُبِّق بموجب قرار المالك Q1 (الخيار A).**
   3. **مواءمة الصياغة المرتبطة بالبند:** بعد تنفيذ الخطوة 3.4 قد يلزم تحديث الصياغة التي تصف موقف ADR-0064 من تعديل وثيقة التجميد. **المصدر الوحيد الذي يحمل العبارة الحرفية** *"performs no freeze-document edit under any reading"* هو ADR-0064 قسم *"Explicitly Out of Scope"* (`0064:1649`)؛ أما `ADR_INDEX.md` (صف 0064) فيستخدم صياغته الخاصة *"without editing them"*، ولا يحمل العبارة الحرفية أعلاه. أي تحديث من هذا القبيل هو **مسألة صياغة/حوكمة**، ولها سابقة في إصلاح قرارات مقبولة عبر **Errata معتمدة من المالك** (سابقة: ADR-0063 Errata E-1…E-5؛ ADR-0051 Erratum E-1؛ مسجَّلة في `docs/governance/frontend/GOVERNANCE_APPROVALS.md`)؛ **ولا يقرّر هذا الـRFC آليةً ملزِمة** ولا ينشئ أي erratum. *(هذه المواءمة ليست جزءًا من هذا الـRFC؛ تُنفَّذ في الخطوة 3 بعد الاعتماد.)*
4. **الخطوة 4 — Implement (مهمة منفصلة ومُصرَّح بها مستقلًا):** لا يبدأ أي تنفيذ برمجي إلا بعد اكتمال الخطوة 3 (`AGENTS.md:310`؛ `ARCHITECTURAL_INVARIANTS.md:69`). التنفيذ يخضع للبوابة الكاملة (`ARCHITECTURE_FREEZE.md:258–261`).
5. **الخطوة 5 — Audit Trail:** الربط المتبادل بين الـRFC والـADR وcommit التنفيذ، وإضافة ملاحظة في `CHANGELOG.md` — `ARCHITECTURE_FREEZE.md:263–265`.

**لا ترحيل بيانات ولا ترحيل مخططات:** لا يخلق هذا المقترح أي عمود/جدول/فهرس/مفتاح خارجي، ولا يغيّر أي هجرة.

---

## 6. Rollback Plan (خطة التراجع)

1. حذف السطر الفرعي المُدرَج تحت §2.4 (السطر 126).
2. التراجع عن إضافة ربط الـRFC في سجل ADR-0064 وعن أي مواءمة لفظية في الخطوة 3.
3. التراجع عن إدخال `CHANGELOG.md` إن أُضيف في الخطوة 5.

**الأثر الصفري:** بما أن التعديل وثائقي بحت، فإن التراجع لا يغيّر أي سلوك تشغيلي؛ تظلّ فقرة *"Immutable closed years"* الأصلية قائمة بكاملها بعد التراجع.

---

## 7. Backward Compatibility (التوافق الرجعي)

* **صفر تغيير سلوكي:** لا يغيّر التعديل أي مسار كتابة أو قراءة، ولا أي دلالات محاسبية لغير حالة الإتلاف المملوك للوحدة.
* **حفظ لكل الضمانات غير التدميرية:** كل تعديل آخر على سنة مغلقة يبقى ممنوعًا؛ §2.3 (التدقيق) و§2.5 (الحتمية) محفوظان بالنص.
* **التوافق مع ADR-0055:83 وADR-0061 §7:** يبقى النص الباقي منهما نافذًا في كل سياق غير تدميري، ومحدودًا فقط داخل وحدة مُتلَفة — كما يوثّق ADR-0064 §7.1/§7.2 (D16).
* **التوافق مع الحوكمة القائمة:** لا يُدخل أي وسم `[arch:allow-*]`، ولا يعدّل `adr_exception_registry.md`، ولا أي أداة تدقيق.

---

## Appendix A — Anchors (مرتكزات موثّقة)

| المرجع | الموقع | الدور |
|---|---|---|
| ADR-0064 | `docs/architecture/0064-unit-export-state-and-owned-data-deletion.md` | §6.1 (D15) الاستثناء التدميري؛ §6.2 (D21) المجموعة المحمية؛ §7.3 تأهيل §2.4؛ §9.1 C-c التوتر المسجَّل حول cache invalidation؛ §4.6/§4.7 preflight وQ6؛ §9.3 Q1–Q6 |
| ARCHITECTURE_FREEZE.md | `docs/architecture/ARCHITECTURE_FREEZE.md` | §2.4 (`:122–129`)؛ §4 (`:224–272`)؛ `:95`, `:226`, `:238–249`, `:251–256`, `:258–261`, `:263–265`, `:335` |
| AGENTS.md | `AGENTS.md` | §8 C1 (`:310`)؛ §12 (`:397–404`؛ مسار الطوارئ فقط)؛ §1 P1/P2/P3/P4 |
| ARCHITECTURAL_INVARIANTS.md | `docs/architecture/ARCHITECTURAL_INVARIANTS.md` | §3 المراجعة والاعتماد (`:57–72`, ولا سيّما `:68–69`) |
| ADR_INDEX.md | `docs/architecture/ADR_INDEX.md` | صف 0064 (`:92`)؛ السياسة (`:112–118`) |
| سابقة مماثلة | `docs/architecture/rfcs/2026-08-09-backup-async-exception.md` + ADR-0043 | نمط تعديل وثيقة التجميد عبر الخطوة 3.4 (§2.6) |

## Appendix B — Step-2 Review Checklist (معايير المراجعة)

وفق `ARCHITECTURE_FREEZE.md:244–249`:

* هل يحافظ التغيير على **الصحّة المحاسبية**؟ (لا يمسّ إغلاق السنة، ولا إعادة تصنيف FIFO، ولا المجموعة المحمية §6.2.)
* هل يحافظ على **قابلية التدقيق**؟ (§2.3 دون تغيير؛ القرار D14 القائم على `audit_log` غير متأثّر.)
* هل يحافظ على **التوافق الرجعي للبيانات التاريخية**؟ (لا تغيير على البيانات التاريخية عمليًا؛ التوافق الوثائقي فقط.)
* هل خطة **الترحيل** كاملة وقابلة للاختبار؟ (تسلسل حوكمي خالٍ من ترحيل البيانات.)
* هل خطة **التراجع** آمنة؟ (حذف سطر فرعي؛ أثر صفري.)

## Appendix C — Explicit Non-Goals (غير الأهداف)

* حلّ/الحكم في **Q2** أو **Q3** أو **Q4** أو **Q5** بأي صيغة.
* اختراع أو اعتماد أي **إجراء إصلاح Q5** عبر الوحدات.
* إعادة فتح أو تعديل قاعدة **Q6** المُصدَّقة (الخيار A — الرفض المغلق-بالفشل).
* تعديل §2.3 أو §2.5 أو أي بند آخر خارج §2.4.
* أي تنفيذ برمجي أو مخططي أو ترحيل.
* افتراض أنّ الـRFC نفسه حرّر وثيقة التجميد مباشرة دون قرار المالك؛ أمّا تعديل §2.4 **فقد وقع فعلًا** بموجب قرار المالك Q1 (الخيار A) واعتمد رسميًا وهو مطابق للمسودة §3.1.
