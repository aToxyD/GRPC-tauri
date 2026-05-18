# مصفوفة التحقق من الادعاءات الفنية (Claim Verification Matrix)

يوفر هذا المستند دليلاً هندسياً ملموساً يربط بين الادعاءات الفنية الكبرى للنظام والتعليمات البرمجية الفعلية والاختبارات التابعة لها، مع توضيح أي حدود تشغيلية بكل أمانة ودقة.

---

## مصفوفة التحقق من الادعاءات

| الادعاء التقني (Claim) | الدليل البرمجي (Code Evidence) | ملفات الاختبارات (Tests) | وثيقة المعمارية (ADR) | الحدود التشغيلية والموانع (Limitations) |
| :--- | :--- | :--- | :--- | :--- |
| **الذرية (Atomic)** | استخدام `with_transaction` في كافة عمليات التعديل والمالية الحساسة للتأكد من نجاح أو فشل الكل. | `src-tauri/src/commands/fiscal.rs` | [ADR 0017](file:///docs/architecture/0017-atomic-secure-restore.md) | تقتصر ذرية المعاملات على قاعدة بيانات SQLite المحلية الفعالة للجهاز فقط. |
| **الحتمية (Deterministic)** | بناء وتطوير حزم المزامنة وهياكل التوقيع ببصمة موحدة بايت مقابل بايت. | `src-tauri/tests/sync_v2_contract_fixtures.rs` | [ADR 0005](file:///docs/architecture/0005-canonical-serialization-contract.md) | تفترض الحتمية ثبات المفتاح التشفيري المشترك وهيكل البيانات المدخلة بشكل تام. |
| **التكرارية الآمنة (Idempotent)** | رفض استيراد الحزم ذات المعرفات المستوردة مسبقاً وتخطي العمليات المكررة. | `src-tauri/tests/sync_idempotency_tests.rs` | [ADR 0014](file:///docs/architecture/0014-sync-conflict-intelligence.md) | تعتمد التكرارية الآمنة على بقاء سجل معرّفات الحزم محفوظاً داخل قاعدة البيانات المحلية. |
| **الإغلاق الآمن عند الفشل (Fail-Closed)** | رفض الوصول الافتراضي للأوامر عند فقدان الصلاحية أو الجلسة أو تعذر الاتصال بالبيانات. | `src/tests/unit/permissions.test.ts` | [ADR 0018](file:///docs/architecture/0018-fail-closed-authorization.md) | يفترض بقاء المكونات الأمنية الأساسية خالية من أخطاء الذاكرة الفادحة في بيئة التشغيل. |
| **الثبات وعدم التعديل (Immutable)** | حظر التعديل على سجلات التدقيق أو العمليات المالية القديمة بعد إغلاق السنة المالية الخاصة بها. | `src/tests/e2e/runtime/sync_fiscal.spec.ts` | [ADR 0002](file:///docs/architecture/0002-sync-package-boundary.md) | يمكن لمشغل يمتلك وصولاً كاملاً لقاعدة البيانات خارج التطبيق تعديل البيانات يدوياً. |
| **كشف التلاعب (Tamper-Evident)** | ربط السجلات التوثيقية بسلسلة تشفيرية تشمل البصمات والمخلفات وتكامل النظام محلياً. | `src-tauri/src/application/services/system_integrity_state_service.rs` | [ADR 0015](file:///docs/architecture/0015-streaming-encryption-and-verification.md) | لا يمنع التلاعب بل يكشفه تلقائياً عند بدء التشغيل أو تنفيذ المعاملات التشغيلية. |
| **التسوية والتطهير (Canonical)** | تنظيف وتسوية مسارات الملفات والمستندات قبل قراءتها أو كتابتها أمنياً. | `src-tauri/src/domain/validation.rs` | [ADR 0009](file:///docs/architecture/0009-sync-package-canonical-json-v2.md) | تعتمد على صحة وثبات خوارزميات نظام التشغيل المحلي في معالجة المسارات والروابط. |
| **العمل دون اتصال (Offline-First)** | عدم وجود أي اتصال بشبكة خارجية أثناء التشغيل العادي والاعتماد الكلي على التخزين المحلي. | `src-tauri/src/main.rs` | [ADR 0020](file:///docs/architecture/0020-single-instance-runtime-enforcement.md) | تتطلب عملية التهيئة والتحديث تنزيلاً يدوياً للملفات والبرمجيات من قنوات معتمدة. |
| **الاستعادة الآمنة (Secure Restore)** | اختبار وتدقيق ملف الاستعادة بالكامل في قاعدة بيانات معزولة ومؤقتة قبل اعتماده. | `src/tests/e2e/runtime/backup.spec.ts` | [ADR 0017](file:///docs/architecture/0017-atomic-secure-restore.md) | تتطلب الاستعادة توفر مساحة تخزين كافية على الجهاز المحلي لإنشاء الملف المؤقت. |
| **نزاهة التدقيق (Audit Integrity)** | كتابة سجل تدقيق متزامن وتلقائي تشفيرياً مع كل عملية تعديل تشغيلية. | `src/tests/integration/login.test.ts` | [ADR 0013](file:///docs/architecture/0013-observability-layer.md) | يمكن تعطيل أوامر التسجيل في حال اختراق نظام التشغيل المحلي بأكمله وحذف الملفات. |
| **ملكية الجلسة (Session Ownership)** | ربط الجلسة بمعرّف فريد وتخزينها في الذاكرة المعزولة للواجهة الخلفية. | `src/tests/unit/session.test.ts` | [ADR 0018](file:///docs/architecture/0018-fail-closed-authorization.md) | يؤدي انتهاء الجلسة أو مسح الذاكرة إلى خروج المستخدم فوراً دون الاحتفاظ بالمدخلات غير المحفوظة. |
| **ضمانات التفويض (Authorization Guarantees)** | التحقق الصارم من الدور ورسم حدود الموارد الفعالة قبل تشغيل الكود في الـ Backend. | `src/tests/integration/login.test.ts` | [ADR 0018](file:///docs/architecture/0018-fail-closed-authorization.md) | يعتمد الأمان على دقة تعيين الأدوار والصلاحيات للمشغلين من قِبل إدارة النظام. |
| **سلامة الحزمة (Package Integrity)** | التحقق من بصمة الحزم والمفتاح التوقيعي التابع للمؤسسة قبل فك التشفير. | `src/tests/e2e/runtime/sync_fiscal.spec.ts` | [ADR 0003](file:///docs/architecture/0003-sync-protocol-versioning-and-integrity.md) | يفترض سلامة وحفظ مفتاح التوقيع الرقمي للمؤسسة بشكل آمن وسري تماماً. |
| **حماية الإعادة (Replay Protection)** | التحقق من صلاحية رموز التنفيذ الفريدة (Tokens) والتأكد من مطابقتها التامة لبصمة النظام الفعالة. | `src/tests/e2e/runtime/sync_fiscal.spec.ts` | [ADR 0002](file:///docs/architecture/0002-sync-package-boundary.md) | تفشل الحماية في حال حذف الذاكرة التوقيتية القصيرة وتكرار الإرسال الفوري للطلب بالملي ثانية. |
| **الأمان التشغيلي (Operational Safety)** | فرض وضع الصيانة وإغلاق قاعدة البيانات الفعالة لتجنب تداخل العمليات أثناء التحديث. | `src-tauri/src/commands/operational.rs` | [ADR 0020](file:///docs/architecture/0020-single-instance-runtime-enforcement.md) | قد تؤدي عمليات الصيانة الطويلة إلى تعليق الواجهات التشغيلية مؤقتاً للمشغلين الفعليين. |

---

## إرشادات التحقق والمطابقة الفنية

يجب على المطورين والمدققين تحديث هذه المصفوفة عند إدخال أي تعديل أمني أو إلحاق منطق عمل جديد بالنظام لضمان بقائها كوثيقة تعاقدية حية وممثلة بدقة للواقع البرمجي الفعلي.
