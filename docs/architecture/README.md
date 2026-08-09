# سجلات القرارات المعمارية (Architectural Decision Records)

يوثق هذا المجلد القرارات المعمارية الجوهرية التي اتخذت أثناء تطوير نظام GRPC، مع توضيح السياق والأسباب والنتائج المترتبة على كل قرار.

| الرقم | العنوان | الحالة |
| :--- | :--- | :--- |
| 0001 | [سياسة استخراج طبقة القراءة (Read Layer Extraction Policy)](0001-read-layer-extraction-policy.md) | مقبول |
| 0002 | [حدود حزمة المزامنة (التطبيق مقابل البنية التحتية)](0002-sync-package-boundary.md) | مقبول |
| 0003 | [إصدار بروتوكول المزامنة ومعايير السلامة](0003-sync-protocol-versioning-and-integrity.md) | مقبول |
| 0004 | [تغييرات البروتوكول تعتبر تغييرات جذرية (Breaking Changes)](0004-protocol-changes-are-breaking-changes.md) | مقبول |
| 0005 | [عقد التسلسل القياسي (Canonical Serialization) لسلامة الحزم](0005-canonical-serialization-contract.md) | مقبول |
| 0006 | [هيكل تدوير مفاتيح التوقيع (Signing Key Rotation)](0006-signing-key-rotation-skeleton.md) | مقبول |
| 0007 | [سياسة نافذة إهمال مفاتيح التوقيع](0007-signing-key-deprecation-window-policy.md) | مقبول |
| 0008 | [هوية الموقع الموثوق (Trusted Signer Identity)](0008-trusted-signer-identity-skeleton.md) | مقبول |
| 0009 | [تنسيق Canonical JSON V2 للسلامة والتواقيع](0009-sync-package-canonical-json-v2.md) | مقبول |
| 0010 | [طبقة النقل الموحدة القائمة حصراً على الحزم](0010-sync-package-only-transport.md) | مقبول |
| 0011 | [الهجرة المعمارية الموحدة (الطبقات النظيفة)](0011-unified-architecture-migration.md) | مقبول |
| 0012 | [سياسة عرض أخطاء الإنتاج (Production Error Exposure)](0012-production-error-exposure-policy.md) | مقبول |
| 0013 | [طبقة قابلية المراقبة (Observability Layer)](0013-observability-layer.md) | مقبول |
| 0014 | [ذكاء معالجة تعارضات المزامنة (Sync Conflict Intelligence)](0014-sync-conflict-intelligence.md) | مقبول |
| 0015 | [التشفير المتدفق والتحقق من التدقيق عبر age](0015-streaming-encryption-and-verification.md) | مقبول |
| 0016 | [مسار المزامنة المراعي لاستهلاك الذاكرة (Memory-Aware)](0016-memory-aware-sync-pipeline.md) | مقبول |
| 0017 | [الاستعادة الآمنة والذرية (Atomic Secure Restore)](0017-atomic-secure-restore.md) | مقبول |
| 0018 | [التفويض المعتمد على سياسة Fail-Closed](0018-fail-closed-authorization.md) | مقبول |
| 0019 | [الاعتماد الحصري على تشفير age](0019-legacy-crypto-isolation.md) | مقبول |
| 0020 | [فرض تشغيل نسخة واحدة](0020-single-instance-runtime-enforcement.md) | مقبول |

الوثائق التشغيلية المكملة:
- [دليل المزامنة (Sync Runbook)](../sync-runbook.md)
- [إجراءات التعامل مع أخطاء الإنتاج](../runbooks/production-error-handling.md)
- [إجراءات أمان مفاتيح الإنتاج](../runbooks/security-production-keys.md)
- [تهيئة المسؤول (Admin Bootstrap)](../runbooks/admin-bootstrap.md)
- [تركيب وتفعيل التراخيص (Licensing Installation)](../runbooks/licensing-installation.md)
- [سلوك ربط الترخيص بالعقدة (Node-A / Node-B Binding)](../runbooks/licensing-node-binding.md)
- [فقدان المفاتيح وإعادة التزويد (Key Loss & Re-Provisioning)](../runbooks/key-loss-and-reprovisioning.md)
- [قائمة التحقق للنشر التجريبي (Trial Deployment Checklist)](../runbooks/trial-deployment-checklist.md)
