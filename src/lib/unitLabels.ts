// SEC-087 Phase 7F: presentation-only label mapping for the backend UnitMeasure
// codes 1..=10 (see `src-tauri/src/domain/units.rs`). Display only — canonical
// codes are the numeric wire contract (A5/F3); the frontend never decides units.

export const UNIT_LABELS: Record<number, string> = {
  1: 'كلغ', 2: 'لتر', 3: 'دلو', 4: 'قارورة', 5: 'صفيحة',
  6: 'قطعة', 7: 'بيضة', 8: 'علبة', 9: 'كيس', 10: 'خبزة',
};

export const UNIT_CODES = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

export function unitLabel(code: number): string {
  return UNIT_LABELS[code] ?? `${code}`;
}