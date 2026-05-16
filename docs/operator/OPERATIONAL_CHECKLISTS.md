# Operational Checklists

### A) Daily Opening Checklist
- [ ] Login with assigned operator credentials.
- [ ] Check System Health dashboard; ensure status is `Healthy`.
- [ ] Review any unread Notifications or Operational Warnings.
- [ ] Verify `Audit Log` for any unauthorized access attempts from previous sessions.

### B) Pre-Close Fiscal Checklist (Wilaya)
- [ ] Run a comprehensive `System Health` test to confirm zero anomalies.
- [ ] Verify all Unit imports for the month are synchronized and successful.
- [ ] Click "Create Backup" and wait for the successful confirmation.
- [ ] Verify there are no pending synchronization packages in the queue.

### C) Pre-Restore Checklist
- [ ] Confirm with administration that a restore is explicitly authorized.
- [ ] Export a fresh backup of the *current* state (even if corrupted) for forensic purposes.
- [ ] Visually verify the date and time of the backup target to ensure correct selection.
- [ ] Have the exact confirmation token ready to type (`RESTORE`).

### D) Post-Restore Validation Checklist
- [ ] Log in with standard credentials.
- [ ] Navigate to the `Audit Log` and confirm the `RESTORE_EXECUTED` event is present.
- [ ] Run the System Health diagnostics to confirm the restored database is `Healthy`.
- [ ] Visually inspect the latest inventory numbers to confirm the rollback target date.

### E) Monthly Integrity Review Checklist
- [ ] Open `Audit Integrity` page.
- [ ] Verify the Audit Chain is fully verified and unbroken.
- [ ] Inspect the `Conflict Center` for unresolved data anomalies.
- [ ] Archive all successfully processed monthly reports.

### F) Before Applying Authorized Fiscal Transition (Unit)
- [ ] Verify the `.pkg` file source is from an authorized Wilaya representative.
- [ ] Ensure the computer's system clock is synchronized to standard time (UTC).
- [ ] Generate a local backup of the Unit prior to applying the transition.
- [ ] Read the confirmation warning fully before typing `APPLY-FISCAL-TRANSITION`.

### G) Before Archiving Fiscal Year
- [ ] Ensure the fiscal close was successfully completed and tested.
- [ ] Verify there are no outstanding transactions for the targeted year.
- [ ] Acknowledge the permanent immutability warning. (Archived years CANNOT be restored to an active state).
