# Projection Purity Lockdown

**Status:** Active — Phase 3B Enforcement

**Version:** v1.0.0

**Effective:** 2026-06-01

---

## 1. Backend Ownership Rules

The backend exclusively owns:

| Domain | Owner |
|--------|-------|
| Accounting semantics | Backend |
| FIFO semantics | Backend |
| Inventory valuation | Backend |
| KPI computation | Backend |
| Cost calculation | Backend |
| Average computation | Backend |
| Line total reconstruction | Backend |
| Aggregate cost reconstruction | Backend |
| Consumption arithmetic | Backend |
| Report generation | Backend |
| Forecast generation | Backend |
| Recommendation generation | Backend |

Any computation in the frontend that mirrors, approximates, or reimplements
backend semantics is an **architecture violation**.

---

## 2. Frontend Projection Rules

### FE-141 — Division on Projection Values

**Severity:** ERROR

**Detection:** `/` followed by projection keywords (cost, average, beneficiaries, quantity, total, price)

**Forbidden:** Dividing projection values to compute percentages, averages, or per-unit metrics.

**Allowed:** CSS calculations (e.g., Tailwind responsive classes), layout math, display-only percentage bars with `[arch:allow-fe141]`.

**Rationale:** Percentage computation embeds business interpretation (e.g., "cost per beneficiary")
that must be computed by the backend.

### FE-142 — Multiplication on Projection Values

**Severity:** ERROR

**Detection:** `*` followed by monetary/quantity keywords (unit_cost, unitCost, price, cost, quantity, amount, beneficiaries)

**Forbidden:** Computing line totals (qty × unit_cost), inventory valuation, cost estimation, consumption estimation.

**Allowed:** Display scaling (e.g., chart height multipliers) with `[arch:allow-fe142]`.

**Rationale:** Monetary arithmetic in the frontend duplicates FIFO and cost-allocation logic
owned by the backend.

### FE-143 — Aggregate Cost Reconstruction

**Severity:** ERROR

**Detection:** `+` followed by cost keywords (cost, total_cost, predicted_fifo_cost)

**Forbidden:** Summing individual costs to reconstruct aggregate totals.

**Allowed:** String concatenation, array concatenation with `[arch:allow-fe143]`.

**Rationale:** The backend already provides `total_amount` and aggregate projections.
Recomputation in the frontend bypasses backend authority.

### FE-145 — Consumption Arithmetic

**Severity:** ERROR

**Detection:** arithmetic operators followed by consumption keywords (consumed, planned, mealCount, meal_count, portion, remaining, portionCount)

**Forbidden:** Computing consumption rates, remaining quantities, portion adjustments.

**Allowed:** Display formatting with `[arch:allow-fe145]`.

**Rationale:** Consumption arithmetic involves operational semantics (spoilage rates,
planned vs actual reconciliation) that only the backend can compute correctly.

### FE-146 — Average Recomputation

**Severity:** ERROR

**Detection:** `average`/`mealAverage`/`dailyAverage`/`avg` followed by `:` or `=`, or division by beneficiaries/count/days

**Forbidden:** Computing averages from projection values.

**Allowed:** Type definitions referencing backend field names with `[arch:allow-fe146]`,
field renames of backend `meal_average` properties, infrastructure telemetry with `[arch:allow-fe146]`.

**Rationale:** Averages (cost per beneficiary, daily consumption rate) are KPI computations
owned exclusively by the backend.

### FE-147 — Projection Reassembly

**Severity:** ERROR

**Detection:** Spread of projection objects (`...project`, `...forecast`, `...report`),
`Object.assign` with projection target

**Forbidden:** Reconstructing aggregate projection objects from raw fields.

**Allowed:** Pure display mapping, field renaming, UI adapter objects without computation
with `[arch:allow-fe147]`.

**Rationale:** Reassembling projections in the frontend creates hidden domain-layer
objects that bypass backend projection contracts.

### FE-148 — Long `$derived` Computation Chains

**Severity:** ERROR

**Detection:** Two or more `$derived`/`$derived.by` calls on the same expression line

**Forbidden:** Multi-stage derivation chains that transform projection data.

**Allowed:** Presentation-only chains (e.g., combining two UI state values) with `[arch:allow-fe148]`.

**Rationale:** Long derivation chains enable hidden semantic logic to migrate into
reactive state, violating projection purity.

---

## 3. Forbidden Arithmetic Catalog

| Operation | Examples | Rule |
|-----------|----------|------|
| Division on cost/average/price | `cost / beneficiaries` | FE-141 |
| Multiplication on monetary values | `qty * unit_cost` | FE-142 |
| Addition on cost values | `cost + total_cost` | FE-143 |
| Arithmetic on consumption | `consumed - planned` | FE-145 |
| Average assignment | `avg = total / count` | FE-146 |
| Projection spread | `{ ...project, ...data }` | FE-147 |
| Derived chain | `$derived($derived(x))` | FE-148 |

---

## 4. Allowed Presentation Catalog

| Operation | Context | Rule |
|-----------|---------|------|
| CSS percentage bars | UI rendering | FE-141 (suppress) |
| Tailwind opacity classes | `bg-red-50/50` | FE-141 (suppress) |
| Chart width multipliers | Display scaling | FE-142 (suppress) |
| String concatenation | Display text | FE-143 (suppress) |
| Display formatting | Number formatting | FE-145 (suppress) |
| Type definitions | TypeScript interfaces referencing backend fields | FE-146 (suppress) |
| Field renames | `mealAverage: m.meal_average` | FE-146 (suppress) |
| Infrastructure metrics | Telemetry latency averages | FE-146 (suppress) |
| Pure display mapping | UI adapters without computation | FE-147 (suppress) |
| Presentation state chains | UI-only `$derived` chains | FE-148 (suppress) |

---

## 5. Suppression Policy

### Tag Format

```
[arch:allow-fe<NNN>] <justification>
```

### Rules

1. Every suppression MUST include a justification text.
2. Empty suppressions (tag without justification) FAIL validation (FE-149).
3. Duplicate identical suppressions on adjacent lines FAIL validation (FE-149).
4. Suppressions that no longer suppress any violation FAIL validation (FE-149).
5. Suppressions are line-local (affect only the tagged line or the following line).
6. File-level suppression is prohibited.
7. Suppressions require review when the suppressed code changes.

### Valid Suppression Example

```
<!-- [arch:allow-fe141] UI percentage bar, not business division -->
<div style="width:{Math.min((count/total)*100,100)}%"></div>
```

### Invalid Suppression Example

```
<!-- [arch:allow-fe141] -->
<div style="width:{count/total}%"></div>
<!-- Empty justification — will fail FE-149 -->
```

---

## 6. Examples

### Forbidden ❌

```ts
// FE-143 violation: frontend reconstructing aggregate cost
const total = items.reduce((sum, i) => sum + i.total_cost, 0);

// FE-142 violation: computing line total
const lineTotal = item.quantity * item.unit_price;

// FE-146 violation: recomputing average
const avg = totalCost / beneficiaries;

// FE-147 violation: reassembling projection
const report = { ...backendReport, computedField: localValue };
```

### Allowed ✅

```ts
// Consume backend projection directly
const total = selectedOrder.total_amount;

// Display-only formatting
const formatted = total.toFixed(2);

// Field rename from backend projection
mealAverage: m.meal_average, // [arch:allow-fe146]
```

---

## 7. Escalation History

| Phase | Date | Change |
|-------|------|--------|
| 3A | 2026-06-01 | FE-141, FE-143, FE-146 upgraded WARNING→ERROR |
| 3A | 2026-06-01 | FE-142, FE-145, FE-147, FE-148 activated as WARNING |
| 3A | 2026-06-01 | Suppression framework introduced |
| 3B | 2026-06-01 | FE-142, FE-145, FE-147, FE-148 upgraded WARNING→ERROR |
| 3B | 2026-06-01 | FE-149 suppression regression detection activated |
| 3B | 2026-06-01 | Projection purity baseline established |
