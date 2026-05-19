#!/bin/sh
# Pre-push hook to run governance CI gates
echo "Running pre-push governance checks..."
bun run ci
if [ $? -ne 0 ]; then
  echo "❌ Push rejected: Governance checks failed."
  exit 1
fi
echo "✅ All checks passed. Proceeding with push."
exit 0
