//! Operational Recommendation Service
//!
//! Produces guided, deterministic recommendations the operator can act on.
//!
//! Hard constraints:
//!   - NEVER performs the recommended action
//!   - NEVER mutates state
//!   - NEVER schedules itself
//!   - Output depends ONLY on the current snapshot of observable counters
//!     (deterministic: same inputs → same recommendations)

use crate::errors::AppError;
use crate::repositories::executor::DbExecutor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationalRecommendation {
    pub priority: RecommendationPriority,
    pub code: String,
    pub title: String,
    pub message: String,
}

/// Thresholds (deterministic constants).
const DB_GROWTH_RECOMMEND_MB: f64 = 500.0; // 500 MB
const DB_VACUUM_STRONG_MB: f64 = 1500.0; // 1.5 GB
const FAILED_IMPORTS_RECOMMEND: i64 = 3;
const FAILED_INTEGRITY_RECOMMEND: i64 = 1;
const STALE_BACKUP_DAYS_WARN: i64 = 7;
const STALE_BACKUP_DAYS_CRITICAL: i64 = 30;
const UNRESOLVED_CONFLICTS_WARN: i64 = 1;
const UNRESOLVED_CONFLICTS_CRITICAL: i64 = 5;

pub struct OperationalRecommendationService<'a> {
    executor: DbExecutor<'a>,
}

impl<'a> OperationalRecommendationService<'a> {
    pub fn new(executor: DbExecutor<'a>) -> Self {
        Self { executor }
    }

    /// Build the recommendation list. Order is deterministic.
    pub fn build_recommendations(&self) -> Result<Vec<OperationalRecommendation>, AppError> {
        let mut recs = Vec::new();

        self.recommend_database_growth(&mut recs)?;
        self.recommend_failed_imports(&mut recs)?;
        self.recommend_failed_integrity(&mut recs)?;
        self.recommend_stale_backups(&mut recs)?;
        self.recommend_unresolved_conflicts(&mut recs)?;

        Ok(recs)
    }

    // ── Database growth ─────────────────────────────────────────────────────
    fn recommend_database_growth(
        &self,
        recs: &mut Vec<OperationalRecommendation>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let (page_count, page_size) = self.executor.system().get_page_stats()?;
        let size_mb = (page_count * page_size) as f64 / 1024.0 / 1024.0;

        if size_mb >= DB_VACUUM_STRONG_MB {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::High,
                code: "REC_DB_GROWTH_HIGH".to_string(),
                title: format!("حجم قاعدة البيانات هو {:.0} ميجابايت", size_mb),
                message:
                    "يوصى بـ: إجراء نسخة احتياطية للصيانة + VACUUM أثناء فترة الخمول. لقد نما حجم قاعدة البيانات بشكل كبير؛ سيؤدي تجميد العمليات لفترة وجيزة إلى استعادة المساحة ومنع التدهور المستقبلي."
                        .to_string(),
            });
        } else if size_mb >= DB_GROWTH_RECOMMEND_MB {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::Medium,
                code: "REC_DB_GROWTH".to_string(),
                title: format!("حجم قاعدة البيانات هو {:.0} ميجابايت", size_mb),
                message: "يوصى بـ: إجراء نسخة احتياطية للصيانة + VACUUM أثناء فترة الخمول."
                    .to_string(),
            });
        }
        Ok(())
    }

    // ── Failed imports ──────────────────────────────────────────────────────
    fn recommend_failed_imports(
        &self,
        recs: &mut Vec<OperationalRecommendation>,
    ) -> Result<(), AppError> {
        let window_start = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

        use crate::repositories::RepositoryProvider;
        let repo = self.executor.anomaly();
        let failed = repo.count_failed_import_events(&window_start)?;
        let stale = repo.count_stale_import_conflicts(&window_start)?;

        let total = failed + stale;
        if total >= FAILED_IMPORTS_RECOMMEND {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::High,
                code: "REC_FAILED_IMPORTS".to_string(),
                title: format!("{} عمليات استيراد فاشلة/محظورة في آخر 30 يومًا", total),
                message:
                    "يوصى بـ: التحقق من اتساق مفتاح التوقيع بين الولاية والوحدة. تأكد من أن السنة المالية مفتوحة على كلا الجانبين وأعد المحاولة يدويًا."
                        .to_string(),
            });
        }
        Ok(())
    }

    // ── Failed integrity verification ───────────────────────────────────────
    fn recommend_failed_integrity(
        &self,
        recs: &mut Vec<OperationalRecommendation>,
    ) -> Result<(), AppError> {
        let window_start = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

        use crate::repositories::RepositoryProvider;
        let failed = self
            .executor
            .anomaly()
            .count_recent_failed_integrity_attempts(&window_start)?;

        if failed >= FAILED_INTEGRITY_RECOMMEND {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::High,
                code: "REC_INTEGRITY_FAILURES".to_string(),
                title: format!("{} عمليات تحقق من السلامة فاشلة في آخر 30 يومًا", failed),
                message:
                    "يوصى بـ: أخذ نسخة احتياطية جديدة، ثم تشغيل فحوصات كاملة للمخزون والسلامة المالية أثناء فترة الخمول. تحقق قبل إجراء أي إغلاق مالي آخر."
                        .to_string(),
            });
        }
        Ok(())
    }

    // ── Stale backups ───────────────────────────────────────────────────────
    fn recommend_stale_backups(
        &self,
        recs: &mut Vec<OperationalRecommendation>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let last_backup = self.executor.audit().get_last_backup_timestamp()?;

        match last_backup {
            None => recs.push(OperationalRecommendation {
                priority: RecommendationPriority::High,
                code: "REC_NO_BACKUP".to_string(),
                title: "لم يتم تسجيل أي نسخة احتياطية بعد".to_string(),
                message:
                    "يوصى بـ: إنشاء نسخة احتياطية مشفرة فورًا وتخزين مفتاح الاسترداد بشكل منفصل."
                        .to_string(),
            }),
            Some(ts) => {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts) {
                    let age = chrono::Utc::now() - dt.with_timezone(&chrono::Utc);
                    let days = age.num_days();
                    if days >= STALE_BACKUP_DAYS_CRITICAL {
                        recs.push(OperationalRecommendation {
                            priority: RecommendationPriority::High,
                            code: "REC_BACKUP_VERY_STALE".to_string(),
                            title: format!("آخر نسخة احتياطية عمرها {} يومًا", days),
                            message:
                                "يوصى بـ: إنشاء نسخة احتياطية جديدة قبل أي عمليات أخرى؛ لا تنتظر دورة الصيانة التالية."
                                    .to_string(),
                        });
                    } else if days >= STALE_BACKUP_DAYS_WARN {
                        recs.push(OperationalRecommendation {
                            priority: RecommendationPriority::Medium,
                            code: "REC_BACKUP_STALE".to_string(),
                            title: format!("آخر نسخة احتياطية عمرها {} يومًا", days),
                            message: "يوصى بـ: إنشاء نسخة احتياطية جديدة في فترة الخمول التالية."
                                .to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    // ── Unresolved sync conflicts ───────────────────────────────────────────
    fn recommend_unresolved_conflicts(
        &self,
        recs: &mut Vec<OperationalRecommendation>,
    ) -> Result<(), AppError> {
        use crate::repositories::RepositoryProvider;
        let unresolved = self.executor.sync_conflicts().count_unresolved()?;

        if unresolved >= UNRESOLVED_CONFLICTS_CRITICAL {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::High,
                code: "REC_CONFLICTS_HIGH".to_string(),
                title: format!("{} تعارضات مزامنة غير محلولة", unresolved),
                message: "يوصى بـ: مراجعة مركز التعارضات وحلها يدويًا قبل دورة الاستيراد التالية."
                    .to_string(),
            });
        } else if unresolved >= UNRESOLVED_CONFLICTS_WARN {
            recs.push(OperationalRecommendation {
                priority: RecommendationPriority::Medium,
                code: "REC_CONFLICTS".to_string(),
                title: format!("{} تعارضات مزامنة غير محلولة", unresolved),
                message: "يوصى بـ: مراجعة إدخالات مركز التعارضات وحلها يدويًا.".to_string(),
            });
        }
        Ok(())
    }
}

impl crate::architecture::Service for OperationalRecommendationService<'_> {}
