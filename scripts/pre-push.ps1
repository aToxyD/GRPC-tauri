Write-Host "Running pre-push governance checks..." -ForegroundColor Cyan
bun run ci
if ($LASTEXITCODE -ne 0) {
  Write-Host "❌ Push rejected: Governance checks failed." -ForegroundColor Red
  exit 1
}
Write-Host "✅ All checks passed. Proceeding with push." -ForegroundColor Green
exit 0
