# تدقيق حوكمة التطوير والدمج المستمر (A4 — CI Governance Audit)

التاريخ: 2026-08-09
المرحلة: A4.1 + A4.2 (تحقيق وتحقق) → A4.3 (تفعيل GitHub CI) → A4.4 (إزالة GitLab وتنظيف التوثيق)
الحالة: مكتمل — التقرير هو المرجع المبرر لقرارات A4.3/A4.4

---

## 1. الملخص التنفيذي

- **كان لا يعمل أي CI تلقائي في المستودع قبل A4.3.** كان خط أنابيب GitLab معطّلًا عبر `when: never` منذ 2026-05-21، وخط GitHub Actions موجود لكنه **يدوي فقط** (`workflow_dispatch`) بلا أي trigger تلقائي (`push` / `pull_request`).
- **بعد A4.3 (commit `675dcf4`)**: أصبح GitHub Actions (`.github/workflows/ci.yml`) هو الـCI المعياري الآلي — يعمل على `push` (كل الفروع) و`pull_request` مع الإبقاء على `workflow_dispatch`، وأُضيف `check:obs` قبل `check:arch`.
- **بعد A4.4**: أُزيل `.gitlab-ci.yml` نهائيًا، وأُعيدت تسمية `GITLAB_GOVERNANCE.md` إلى `GITHUB_GOVERNANCE.md`، وحُدّثت كل مراجع GitLab في الوثائق إلى GitHub Actions.
- **`release_certification.ts` ليس هو البوابة المعيارية (canonical gate).** المرجع المعياري هو AGENTS.md §9/§11، وCI الكامل هو الذي يجمع كل البوابات.
- **التحقق على Rust 1.94.0 (نفس إصدار CI):**
  | البوابة | النتيجة |
  |---------|---------|
  | `cargo fmt --check` | ❌ **FAIL** — 44 ملفًا / 221 hunk |
  | `cargo clippy -- -D warnings` | ✅ PASS |
  | `cargo test` | ✅ PASS — 1361 اختبارًا |
  | `bun run check:arch` | ✅ PASS — 0 تحذيرات |
  | `bun run check:obs` | ✅ PASS |
  | `bun run check` (svelte-check) | ✅ PASS — 0 أخطاء / 0 تحذيرات |
  | `bun x vitest run` | ✅ PASS — 88 اختبارًا / 16 ملفًا |
  | `scripts/check_secrets.ts` | ✅ PASS |
  | `scripts/check_docs_governance.ts` | ✅ PASS — 100% |
  | `scripts/check_release_integrity.ts` | ✅ PASS — 100% |
  | `bun audit` | ⚠️ FAIL لكنه مسموح (allow_failure) وفق سياسة DEPENDENCY_GOVERNANCE.md |

- **نتيجة حاسمة:** اختلاف `cargo fmt --check` **مطابق حرفيًا** بين rustfmt 1.8.0 (Rust 1.94) وrustfmt 1.9.0 (Rust 1.96) — أي أن مشكلة الـ44 ملفًا هي **ديون تنسيق حقيقية في المستودع** وليست toolchain drift.

---

## 2. السبب الجذري لتعطّل CI

| الدليل | القيمة |
|--------|--------|
| Commit | `36146cb` — "ci: disable GitLab CI pipeline." (2026-05-21) |
| التغيير | 4 أسطر فقط: `workflow: rules: - when: never` بلا سبب موثق |
| Commit لاحق | `6ce2ecb` — "Add GitHub Actions CI workflow from GitLab CI config" (2026-05-21، بعد التعطيل) |
| الـremote | `github.com/aToxyD/GRPC-tauri.git` (GitHub) — وليس GitLab |

**النتيجة المركّبة:**
1. المشروع انتقل من GitLab إلى GitHub في 2026-05-21.
2. تم تعطيل GitLab CI بدلًا من حذفه، دون توثيق السبب.
3. أُضيف GitHub Actions في اليوم نفسه، لكنه فعّل `workflow_dispatch` **فقط**.
4. النتيجة: **صفر CI تلقائي** — لا GitLab (ميت) ولا GitHub (يدوي).

---

## 3. تحديد البوابة المعيارية (Canonical Gate)

### 3.1 المرجع المعياري (AGENTS.md §9/§11)
البنود المعتمدة لتأهيل الإصدار:
`check:arch` (صفر تحذيرات) + svelte-check + clippy + `cargo test` + vitest + `check_secrets` + `check_docs_governance` + `check_release_integrity` + CI pipeline كبوابة شاملة.

### 3.2 أدوات مقارنة بالـcanonical set

| الأداة | يغطي | لا يغطي | الحكم |
|--------|------|---------|-------|
| `.gitlab-ci.yml` | secrets، frontend build، svelte+vitest، fmt+clippy+check+test، arch، audits، docs، integrity، e2e | — | أقرب إلى الكامل، لكنه **معطّل** |
| `.github/workflows/ci.yml` | نفس GitLab تقريبًا | `check:obs` (مفقود)، triggers تلقائية | موجود لكنه يدوي + ناقص `check:obs` |
| `scripts/run_ci.ts` | check:obs، arch، svelte، vitest، cargo build، e2e | clippy، fmt، cargo test، secrets، docs gov، release integrity | **ليس** البوابة الكاملة رغم تسميته "Comprehensive" في governance_review.md §1.4 |
| `scripts/release_certification.ts` | cargo test، clippy، fmt، arch، determinism، exception governance/expiry | vitest، svelte، secrets، docs gov، release integrity، e2e، tauri build | **أداة مساعدة (supplementary)** لتأهيل الإصدار — ليست canonical |

### 3.3 الاستنتاج
- الـcanonical gate هو **CI الكامل** (GitHub Actions مستقبلًا)، وليس `release_certification.ts`.
- `release_certification.ts` يبقى أداة تأهيل إصدار مكملة (تغطي backend فقط).
- `run_ci.ts` هو بوابة محلية مفيدة لكنه أدنى من الكامل، والتسمية "Comprehensive" في `governance_review.md` §1.4 **غير دقيقة** ويجب مراجعتها في A4.3.

---

## 4. التحقق على Rust 1.94.0 (نفس بيئة CI)

الأداة المثبتة: `rustup toolchain install 1.94.0` → rustc 1.94.0 / rustfmt 1.8.0 / cargo 1.94.0
(المحلي الافتراضي: rustc 1.96.0 / rustfmt 1.9.0)

### 4.1 `cargo fmt --check` — ❌ FAIL
- **Rust 1.94 (rustfmt 1.8.0):** خروج 1 — **221 hunk في 44 ملفًا**
- **Rust 1.96 (rustfmt 1.9.0):** خروج 1 — **221 hunk في 44 ملفًا**
- `diff` بين مخرجَي الفحصين: **مطابق حرفيًا (identical)**.
- **الخلاصة:** المشكلة **ليست toolchain drift**. الكود لم يُنسّق وفق قواعد rustfmt المعتمد في كلا الإصدارين. الملفات تتضمن: إعادة ترتيب `use`، أقواس match-arm، كسر السلاسل الطويلة.
- لا يوجد `rustfmt.toml` / `.rustfmt.toml` / `rust-toolchain` في أي مستوى يثبّت أسلوبًا مخصصًا.

### 4.2 `cargo clippy -- -D warnings` — ✅ PASS
- خروج 0 — صفر تحذيرات على Rust 1.94.

### 4.3 `cargo test` — ✅ PASS
- خروج 0 — **1361 اختبارًا نجح، 0 فشل، 0 تجاهل** (يشمل unit + integration + doc-tests).

### 4.4 بوابات Bun — ✅ PASS (ما عدا bun audit)
تم تشغيل كل بوابات الـcanonical set محليًا بنجاح؛ الوحيد الفاشل `bun audit` (ثغرات `undici` بشكل أساسي) لكنه مصنّف في CI على أنه `allow_failure` / `continue-on-error` بموجب `docs/technical/DEPENDENCY_GOVERNANCE.md`.

---

## 5. فجوة معايير التأهيل في A1/A2/A3

| البوابة | A1/A2/A3 الفعلي | نتيجة A4.2 |
|---------|-----------------|------------|
| `check:arch` | ✅ (صفر تحذيرات) | ✅ |
| `check_docs_governance` | ✅ (100%) | ✅ |
| A2 E2E (Release binary عبر WebKitWebDriver) | ✅ 3/3 | (لم يُعد في A4 — متطلب منفصل) |
| `cargo test` | ❌ لم يُشغَّل | ✅ 1361 |
| `clippy -D warnings` | ❌ لم يُشغَّل | ✅ |
| `cargo fmt --check` | ❌ لم يُشغَّل | ❌ **فشل — 44 ملفًا** |
| `check_secrets` | ❌ لم يُشغَّل | ✅ |
| `check_release_integrity` | ❌ لم يُشغَّل | ✅ |

**الاستنتاج:** معايير تأهيل A-series كانت **مجموعة فرعية** من الـcanonical set؛ ولم يكن `fmt --check` ناجحًا عبر التاريخ الحديث. أي إعادة تفعيل CI سترصد فشل fmt فورًا.

---

## 6. انحراف GitHub Actions عن GitLab (Drift)

| البند | `.gitlab-ci.yml` | `.github/workflows/ci.yml` |
|------|------------------|-----------------------------|
| `check:obs` قبل `check:arch` | ✅ (أُضيف في `cd49870` لحل FE-165) | ❌ **مفقود** |
| Trigger | معطّل (`when: never`) | يدوي فقط (`workflow_dispatch`) |
| صورة backend | `rust:1.94` مباشرة | `ghcr.io/atoxyd/grpc-tauri:ci-latest` (مبني على rust:1.94 + bun + mold) |
| e2e | `playwright:v1.60.0-jammy` + Rust مخصص | `ci-latest` + `playwright install chromium` |
| audit jobs | bun + cargo (كلاهما allow_failure/ignore موثق) | bun (`continue-on-error`) + cargo (نفس الـignores) |

`ci.yml` **متأخر عن** الـgitlab بعد `cd49870` (نقص `check:obs`). أي تفعيل مستقبلي يجب مزامنته أولًا.

---

## 7. فجوة تغطية E2E (CI-E2E ≠ شهادة A2)

- **CI E2E (Linux):** محرك `ViteDriver` — يختبر واجهة `bun run dev` في المتصفح، **لا يختبر ثنائي Tauri الحقيقي** (المحرك الفعلي `WebKitTauriDriver`/`TauriDriver` يُختار فقط على win32 حسب `fixtures/tauriApp.ts`).
- **A2 certification:** استخدم **ثنائي Release الحقيقي** عبر `WebKitWebDriver` مع أدلة `[A2-EVIDENCE]` — أقوى وأكثر تمثيلًا.
- **الاستنتاج:** إعادة تفعيل CI-E2E لا يُغني عن شهادة release-binary من طراز A2. يجب توثيق الاثنين كمسارات منفصلة (A4.3).

---

## 8. وثائق متأثرة بقرار التخلي عن GitLab (مرجع لـA4.4)

> **حالة §8: أُغلقت في A4.4** — كل المراجع أدناه حُدّثت بالفعل؛ الجدول يُبقى كسجل تاريخي للتناقضات التي دفعت إلى القرار.

| المستند | التناقض قبل A4.4 | الإجراء المتخذ في A4.4 |
|---------|------------------|------------------------|
| `docs/technical/CI_CD_GOVERNANCE.md` | يصف GitLab 7 طبقات كـ"جدار ناري" ويشترط "100% نجاح pipeline للدمج" | أُعيدت كتابته لوصف GitHub Actions (7 طبقات) والإشارة إلى قرار A4.3 |
| `docs/technical/GITLAB_GOVERNANCE.md` | حماية فروع GitLab، وسوم تطلق pipeline كاملة | أُعيدت تسميته إلى `GITHUB_GOVERNANCE.md` (عبر `git mv`) مع حوكمة GitHub |
| `docs/architecture/ARCHITECTURE_FREEZE.md` §7 | يسرد `.gitlab-ci.yml` كـ"CI Pipeline" | حُدّث إلى `.github/workflows/ci.yml` |
| `AGENTS.md` §11/§13 | يسرد `.gitlab-ci.yml` كـ"Full gate" | حُدّث إلى `.github/workflows/ci.yml` |
| `docs/architecture/governance_review.md` §1.4 | يسمي `run_ci.ts` "Comprehensive Governance CI Gate" | أعيد تسميته "Local CI Gate (subset)" مع الإشارة لـ`ci_governance_audit.md` |

**ملاحظة تحقق (أُعيد التحقق في A4.4):** لا توجد أداة حوكمة (`check_arch` / `check_release_integrity` / `check_docs_governance`) تفرض وجود `.gitlab-ci.yml`، ولا يوجد أي مرجع تنفيذي آخر يعتمد عليه — فالتخلص منه (A4.4) لم يعارض أدوات الحوكمة، والفحوصات النهائية مرت بنجاح (راجع قسم الفحوصات أدناه).

---

## 9. الاستنتاجات والتوصيات المعلّقة

### ثابت الآن (نتائج A4)
1. ~~لا CI تلقائي — GitLab ميت، GitHub يدوي.~~ → **أُنجز في A4.3 (commit `675dcf4`)**: GitHub Actions هو الـCI الآلي المعياري (`push` + `pull_request` + `workflow_dispatch`)، و`check:obs` أصبح قبل `check:arch`.
2. `release_certification.ts` = أداة مساعدة وليست canonical.
3. البوابة الوحيدة الرافضة حاليًا هي `cargo fmt --check` (44 ملفًا / 221 hunk) — وهي **ديون تنسيق حقيقية** (متطابقة بين rustfmt 1.8.0 و1.9.0).
4. كل بوابات الـcanonical set الأخرى تمر على Rust 1.94.0.
5. ~~GH↔GitLab drift: `ci.yml` ينقصه `check:obs`.~~ → **أُزيلت الفجوة في A4.3**؛ وGitLab أُزيل نهائيًا في A4.4.
6. CI-E2E لا يعادل شهادة A2 على الثنائي الحقيقي — ميّزناها بوضوح في `ci.yml` كتعليق، ويبقى A2 release-binary certification مسارًا منفصلًا.

### منفَّذ
- **A4.3** (`675dcf4`): تفعيل GitHub Actions كـCI معياري آلي + `check:obs` + توثيق فصل E2E في `ci.yml`.
- **A4.4**: حذف `.gitlab-ci.yml`، إعادة تسمية `GITLAB_GOVERNANCE.md` → `GITHUB_GOVERNANCE.md`، تحديث كل مراجع GitLab في AGENTS.md / ARCHITECTURE_FREEZE.md / governance_review.md / CI_CD_GOVERNANCE.md / PIPELINE_FAILURE_GUIDE.md، وتحديث هذا التقرير.

### معلّق (قرار مستقل — ليس جزءًا من A4)
- **إصلاح ديون التنسيق (44 ملفًا / 221 hunk)**: يجب أن يكون التزامًا مستقلًا قابلاً للمراجعة، بعد اعتماد GitHub-only CI. هذه البوابة هي الوحيدة الرافضة حاليًا في الـcanonical set.

---

## 10. أدلة التحقق

| القياس | المسار |
|--------|--------|
| fmt Rust 1.94 | `/tmp/opencode/a4_fmt_194.log` (خروج 1، 221 hunk) |
| fmt Rust 1.96 | `/tmp/opencode/a4_fmt_196.log` (خروج 1، 221 hunk) |
| diff المخرجين | `/tmp/opencode/a4_fmt_diff.log` (مطابق) |
| clippy Rust 1.94 | `/tmp/opencode/a4_clippy_194.log` (خروج 0) |
| cargo test Rust 1.94 | `/tmp/opencode/a4_test_194.log` (خروج 0، 1361 pass) |
| check:obs / check:arch | `/tmp/opencode/a4_obs.log` / `a4_arch.log` (خروج 0) |
| secrets / docs / integrity | `/tmp/opencode/a4_secrets.log` / `a4_docs.log` / `a4_integrity.log` (خروج 0) |
| svelte-check / vitest | `/tmp/opencode/a4_svelte.log` / `a4_vitest.log` (خروج 0) |
| bun audit | `/tmp/opencode/a4_bunaudit.log` (خروج 1 — مسموح) |

---
