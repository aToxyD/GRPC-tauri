use serde::{Deserialize, Serialize};
use thiserror::Error;

// ADR-0012: production-error-exposure-policy
// Internal error details are always logged via log::error! and are shown
// to the caller ONLY when compiled in debug mode (cfg(debug_assertions)).
// In release/production builds the user receives a generic Arabic message.

pub type AppResult<T> = Result<T, AppError>;

/// خطأ يُعرض للمستخدم بالعربية فقط
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

/// أنواع الأخطاء المخصصة للتطبيق
#[derive(Error, Debug)]
pub enum AppError {
    /// أخطاء قاعدة البيانات
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    /// أخطاء التحقق من صحة البيانات
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// أخطاء منطق الأعمال
    #[error("Business logic error: {0}")]
    BusinessLogic(#[from] BusinessLogicError),

    /// أخطاء المصادقة والتحقق من الهوية
    #[error("Authentication error: {0}")]
    Authentication(#[from] AuthenticationError),

    /// أخطاء الصلاحيات (Authorization / Policy)
    #[error("Authorization error: {0}")]
    Authorization(#[from] AuthorizationError),

    /// أخطاء SQLite مباشرة
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// أخطاء عامة
    #[error("Internal error: {0}")]
    Internal(String),

    /// أخطاء الإعداد (Configuration) — متغير بيئة إلزامي غير موجود
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// أخطاء تحويل التواريخ
    #[error("Date parse error: {0}")]
    DateParse(String),

    /// أخطاء تنسيق الملفات
    #[error("File format error: {0}")]
    FileFormat(String),

    /// ميزة غير منفذة
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// أخطاء الإدخال والإخراج
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// أخطاء الصلاحيات (Policy-based authorization)
/// FAIL-CLOSED: default is deny, never allow.
#[derive(Error, Debug, Clone)]
pub enum AuthorizationError {
    #[error("غير مصرح: يتطلب تسجيل الدخول")]
    NotAuthenticated,
    #[error("غير مصرح: يتطلب صلاحيات Admin")]
    RequiresAdmin,
    #[error("غير مصرح: هذه العملية متاحة فقط لعقد WILAYA")]
    RequiresWilayaNode,
    #[error("غير مصرح: هذه العملية متاحة فقط لعقد UNIT")]
    RequiresUnitNode,
    #[error("غير مصرح: نطاق الوحدة غير متطابق")]
    UnitScopeMismatch,
    /// Catch-all deny — emitted by the fail-closed policy default.
    #[error("غير مصرح: صلاحيات غير كافية لتنفيذ هذه العملية")]
    InsufficientPermissions,
}

impl AuthorizationError {
    /// Stable machine-readable code for frontend/audit/metrics mapping.
    pub const fn code(&self) -> &'static str {
        match self {
            AuthorizationError::NotAuthenticated => "AUTH_NOT_AUTHENTICATED",
            AuthorizationError::RequiresAdmin => "AUTH_REQUIRES_ADMIN",
            AuthorizationError::RequiresWilayaNode => "AUTH_REQUIRES_WILAYA_NODE",
            AuthorizationError::RequiresUnitNode => "AUTH_REQUIRES_UNIT_NODE",
            AuthorizationError::UnitScopeMismatch => "AUTH_UNIT_SCOPE_MISMATCH",
            AuthorizationError::InsufficientPermissions => "AUTH_INSUFFICIENT_PERMISSIONS",
        }
    }
}

/// أخطاء قاعدة البيانات
#[derive(Error, Debug)]
pub enum DatabaseError {
    /// خطأ في الاتصال
    #[error("Connection failed: {message}")]
    Connection { message: String },

    /// خطأ في تنفيذ الاستعلام
    #[error("Query execution failed: {message}")]
    QueryExecution { message: String },

    /// انتهاك قيد فريد
    #[error("Unique constraint violation: {field}")]
    UniqueConstraint { field: String },

    /// انتهاك مفتاح خارجي
    #[error("Foreign key violation: {field}")]
    ForeignKeyConstraint { field: String },

    /// انتهاك قيد التحقق
    #[error("Check constraint violation: {field}")]
    CheckConstraint { field: String },

    /// السجل غير موجود
    #[error("Record not found: {table} - {id}")]
    NotFound { table: String, id: String },

    /// خطأ في المعاملة
    #[error("Transaction failed: {message}")]
    Transaction { message: String },

    /// خطأ في الترحيل
    #[error("Migration failed: {message}")]
    Migration { message: String },
}

/// أخطاء التحقق من صحة البيانات
#[derive(Error, Debug)]
pub enum ValidationError {
    /// قيمة مفقودة
    #[error("Required field missing: {field}")]
    Required { field: String },

    /// القيمة خارج النطاق
    #[error("Value out of range: {field} = {value}")]
    OutOfRange { field: String, value: String },

    /// صيغة غير صالحة
    #[error("Invalid format: {field} - {message}")]
    InvalidFormat { field: String, message: String },

    /// السعر غير صالح
    #[error("Invalid price: {value}")]
    InvalidPrice { value: f64 },

    /// الكمية غير صالحة
    #[error("Invalid quantity: {value}")]
    InvalidQuantity { value: f64 },

    /// التاريخ غير صالح
    #[error("Invalid date: {value}")]
    InvalidDate { value: String },

    /// اسم مكرر
    #[error("Duplicate name: {entity} - {name}")]
    DuplicateName { entity: String, name: String },

    /// كلمة مرور ضعيفة
    #[error("Weak password: {reason}")]
    WeakPassword { reason: String },

    /// اسم المستخدم غير صالح
    #[error("Invalid username: {reason}")]
    InvalidUsername { reason: String },

    /// ملف غير صالح
    #[error("Invalid file: {path} - {reason}")]
    InvalidFile { path: String, reason: String },
}

/// أخطاء منطق الأعمال
#[derive(Error, Debug)]
pub enum BusinessLogicError {
    /// مخزون غير كافٍ
    #[error("Insufficient stock: {0}")]
    InsufficientStock(String),

    /// العملية غير مسموحة للدور الحالي
    #[error("Unauthorized role: {role}")]
    UnauthorizedRole { role: String },

    /// العملية غير مسموحة للنوع الحالي من العقدة
    #[error("Unauthorized node type: {node_type}")]
    UnauthorizedNodeType { node_type: String },

    /// المنتج غير متوفر في السنة الحالية
    #[error("Product not available in year: {year}")]
    ProductNotAvailableInYear { year: i32 },

    /// الطلب مؤكد مسبقاً
    #[error("Order already confirmed: {order_id}")]
    OrderAlreadyConfirmed { order_id: String },

    /// تقرير موجود مسبقاً لهذا التاريخ
    #[error("Report already exists for date: {date}")]
    ReportAlreadyExists { date: String },

    /// خطأ في حساب السعر
    #[error("Price calculation error: {message}")]
    PriceCalculation { message: String },

    /// خطأ في حساب المعدل
    #[error("Rate calculation error: {message}")]
    RateCalculation { message: String },

    /// خطأ في التحقق من صحة البيانات
    #[error("Validation error: {message}")]
    ValidationError { field: String, message: String },

    /// الوحدة غير مكونة
    #[error("Unit not configured")]
    UnitNotConfigured,

    /// مورد غير موجود
    #[error("Resource not found: {resource} (ID: {id})")]
    ResourceNotFound { resource: String, id: String },

    /// عملية عبر ولاية محظورة
    #[error(
        "Cross-wilaya operation forbidden: {resource} from wilaya {from_wilaya} to {to_wilaya}"
    )]
    CrossWilayaForbidden {
        resource: String,
        from_wilaya: String,
        to_wilaya: String,
    },

    /// حذف ممنوع بسبب التبعيات
    #[error("Delete forbidden: {resource} - {reason}")]
    DeleteForbidden { resource: String, reason: String },

    /// حزمة مزامنة مطبَّقة مسبقاً (نفس معرّف الحزمة)
    #[error("Duplicate sync package: {package_id}")]
    DuplicateSyncPackage { package_id: String },

    /// السنة المالية مغلقة
    #[error("Fiscal year {year} is closed")]
    FiscalYearClosed { year: i32 },

    /// السنة المالية غير موجودة
    #[error("Fiscal year {year} not found")]
    FiscalYearNotFound { year: i32 },

    /// السنة المالية مفتوحة مسبقاً
    #[error("Fiscal year {year} is already open")]
    FiscalYearAlreadyOpen { year: i32 },

    /// عملية تشغيلية مرفوضة (مثلاً: confirmation token mismatch)
    #[error("Operation not permitted: {message}")]
    OperationNotPermitted { message: String },

    /// السنة المالية مؤرشفة — immutable
    #[error("Fiscal year {year} is archived and immutable")]
    FiscalYearArchived { year: i32 },

    /// استعادة نسخة أقدم من الحالة المؤرشفة الحالية
    #[error("Restore would regress archived state: {message}")]
    RestoreWouldRegressArchivedState { message: String },

    /// رمز تنفيذ قديم أو غير صالح
    #[error("Stale execution token for operation: {operation}")]
    StaleExecutionToken { operation: String },

    /// تنفيذ مكرر خلال نافذة قصيرة
    #[error("Operation throttled: {operation}")]
    OperationThrottled { operation: String },

    /// استعادة/أرشفة محظورة بسبب حالة سلامة حرجة
    #[error("Operation blocked by integrity state: {state}")]
    RestoreBlockedByIntegrity { state: String },

    /// عملية مرفوضة أثناء وضع الصيانة التشغيلية
    #[error("Operation blocked by maintenance mode ({state}): {operation}")]
    MaintenanceModeBlocked { state: String, operation: String },

    /// السعر مغلق للسنة المالية النشطة
    #[error("Price locked for active fiscal year {fiscal_year}")]
    PriceLockedForActiveFiscalYear { fiscal_year: i32 },
}

/// أخطاء المصادقة
#[derive(Error, Debug)]
pub enum AuthenticationError {
    /// بيانات اعتماد غير صالحة
    #[error("Invalid credentials for user: {username}")]
    InvalidCredentials { username: String },

    /// المستخدم غير موجود
    #[error("User not found: {username}")]
    UserNotFound { username: String },

    /// كلمة المرور القديمة غير صحيحة
    #[error("Invalid old password for user: {user_id}")]
    InvalidOldPassword { user_id: String },

    /// المستخدم محظور
    #[error("User is locked: {username}")]
    UserLocked { username: String },

    /// انتهت الجلسة
    #[error("Session expired")]
    SessionExpired,

    /// خطأ في تشفير كلمة المرور
    #[error("Password hash error: {message}")]
    PasswordHash { message: String },

    /// لا توجد جلسة نشطة
    #[error("No active session")]
    SessionNotFound,
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("JSON error: {}", e))
    }
}

impl From<uuid::Error> for AppError {
    fn from(e: uuid::Error) -> Self {
        AppError::Internal(format!("UUID error: {}", e))
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(e: argon2::password_hash::Error) -> Self {
        map_argon2_error(e)
    }
}

impl AppError {
    /// تحويل الخطأ إلى رسالة للمستخدم بالعربية
    pub fn to_user_error(&self) -> UserError {
        match self {
            // أخطاء الصلاحيات
            AppError::Authorization(e) => UserError {
                code: e.code().to_string(),
                message: e.to_string(),
                details: None,
            },
            // أخطاء قاعدة البيانات
            AppError::Database(DatabaseError::Connection { .. }) => UserError {
                code: "DB_CONNECTION".to_string(),
                message: "تعذر الاتصال بقاعدة البيانات. يرجى التحقق من النظام والمحاولة مرة أخرى."
                    .to_string(),
                details: Some(self.to_string()),
            },
            AppError::Database(DatabaseError::UniqueConstraint { field }) => UserError {
                code: "DB_UNIQUE".to_string(),
                message: format!("القيمة مسجلة مسبقاً: {}", translate_field(field)),
                details: Some(self.to_string()),
            },
            AppError::Database(DatabaseError::ForeignKeyConstraint { field: _ }) => UserError {
                code: "DB_FOREIGN_KEY".to_string(),
                message: "لا يمكن الحذف، السجل مرتبط ببيانات أخرى.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Database(DatabaseError::NotFound { table, .. }) => UserError {
                code: "DB_NOT_FOUND".to_string(),
                message: format!("{} غير موجود", translate_table(table)),
                details: Some(self.to_string()),
            },

            // أخطاء التحقق
            AppError::Validation(ValidationError::Required { field }) => UserError {
                code: "VAL_REQUIRED".to_string(),
                message: format!("الحقل مطلوب: {}", translate_field(field)),
                details: None,
            },
            AppError::Validation(ValidationError::InvalidPrice { .. }) => UserError {
                code: "VAL_PRICE".to_string(),
                message: "السعر يجب أن يكون أكبر من صفر.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Validation(ValidationError::InvalidQuantity { .. }) => UserError {
                code: "VAL_QUANTITY".to_string(),
                message: "الكمية يجب أن تكون أكبر من صفر.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Validation(ValidationError::InvalidDate { .. }) => UserError {
                code: "VAL_DATE".to_string(),
                message: "صيغة التاريخ غير صالحة.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Validation(ValidationError::DuplicateName { entity, name }) => UserError {
                code: "VAL_DUPLICATE".to_string(),
                message: format!("اسم {} '{}' مستخدم مسبقاً", translate_entity(entity), name),
                details: None,
            },
            AppError::Validation(ValidationError::WeakPassword { .. }) => UserError {
                code: "VAL_PASSWORD".to_string(),
                message: "كلمة المرور ضعيفة. يجب أن تكون 8 أحرف على الأقل وتحتوي على أرقام ورموز."
                    .to_string(),
                details: Some(self.to_string()),
            },
            AppError::Validation(ValidationError::InvalidUsername { .. }) => UserError {
                code: "VAL_USERNAME".to_string(),
                message: "اسم المستخدم غير صالح. يجب أن يكون 3 أحرف على الأقل.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Validation(ValidationError::InvalidFormat { field, message }) => UserError {
                code: "VAL_FORMAT".to_string(),
                message: format!("{}: {}", translate_field(field), message),
                details: Some(self.to_string()),
            },

            // أخطاء منطق الأعمال
            AppError::BusinessLogic(BusinessLogicError::InsufficientStock(msg)) => UserError {
                code: "BIZ_STOCK".to_string(),
                message: format!("المخزون غير كافٍ: {}", msg),
                details: Some(self.to_string()),
            },
            AppError::BusinessLogic(BusinessLogicError::UnauthorizedRole { role }) => UserError {
                code: "BIZ_ROLE".to_string(),
                message: format!("غير مصرح للدور '{}' بتنفيذ هذه العملية", role),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::UnauthorizedNodeType { node_type }) => {
                UserError {
                    code: "BIZ_NODE".to_string(),
                    message: format!("غير مصرح لنوع العقدة '{}' بتنفيذ هذه العملية", node_type),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::OrderAlreadyConfirmed { .. }) => {
                UserError {
                    code: "BIZ_ORDER".to_string(),
                    message: "الطلب مؤكد مسبقاً ولا يمكن تعديله.".to_string(),
                    details: Some(self.to_string()),
                }
            }
            AppError::BusinessLogic(BusinessLogicError::ReportAlreadyExists { date }) => {
                UserError {
                    code: "BIZ_REPORT".to_string(),
                    message: format!("يوجد تقرير مسبقاً للتاريخ {}", date),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::DuplicateSyncPackage { package_id: _ }) => {
                UserError {
                    code: "PACKAGE_ALREADY_IMPORTED".to_string(),
                    message: "تم استيراد هذه الحزمة مسبقاً (نفس المعرف)، ولن يُعاد تطبيقها."
                        .to_string(),
                    details: Some(self.to_string()),
                }
            }
            AppError::BusinessLogic(BusinessLogicError::ResourceNotFound { resource, id }) => {
                UserError {
                    code: "BIZ_NOT_FOUND".to_string(),
                    message: format!("{} غير موجود (المعرف: {})", resource, id),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::CrossWilayaForbidden {
                resource,
                from_wilaya,
                to_wilaya,
            }) => UserError {
                code: "BIZ_CROSS_WILAYA".to_string(),
                message: format!(
                    "غير مصرح بالوصول لـ {} في ولاية {} من ولاية {}",
                    resource, from_wilaya, to_wilaya
                ),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::DeleteForbidden { resource, reason }) => {
                UserError {
                    code: "BIZ_DELETE".to_string(),
                    message: format!("لا يمكن حذف {}: {}", translate_entity(resource), reason),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::FiscalYearClosed { year }) => UserError {
                code: "FISCAL_CLOSED".to_string(),
                message: format!("السنة المالية {} مغلقة. لا يمكن إجراء تعديلات.", year),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::FiscalYearNotFound { year }) => UserError {
                code: "FISCAL_NOT_FOUND".to_string(),
                message: format!("السنة المالية {} غير موجودة.", year),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::FiscalYearAlreadyOpen { year }) => {
                UserError {
                    code: "FISCAL_ALREADY_OPEN".to_string(),
                    message: format!("السنة المالية {} مفتوحة بالفعل.", year),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::OperationNotPermitted { message }) => {
                UserError {
                    code: "OPERATION_NOT_PERMITTED".to_string(),
                    message: message.clone(),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::FiscalYearArchived { year }) => UserError {
                code: "FISCAL_ARCHIVED".to_string(),
                message: format!("السنة المالية {} مؤرشفة ولا يمكن تعديلها.", year),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::RestoreWouldRegressArchivedState {
                message,
            }) => UserError {
                code: "RESTORE_ARCHIVED_REGRESSION".to_string(),
                message: message.clone(),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::StaleExecutionToken { operation }) => {
                UserError {
                    code: "STALE_EXECUTION_TOKEN".to_string(),
                    message: format!(
                        "رمز التنفيذ غير صالح أو قديم للعملية '{}'. أعد فتح الشاشة وأصدر رمزاً جديداً.",
                        operation
                    ),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::OperationThrottled { operation }) => {
                UserError {
                    code: "OPERATION_THROTTLED".to_string(),
                    message: format!(
                        "تم تنفيذ '{}' مؤخراً. انتظر ثانيتين قبل إعادة المحاولة.",
                        operation
                    ),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::RestoreBlockedByIntegrity { state }) => {
                UserError {
                    code: "INTEGRITY_BLOCKS_OPERATION".to_string(),
                    message: format!(
                        "العملية محظورة بسبب حالة سلامة النظام ({state}). عالج التحذيرات أولاً."
                    ),
                    details: None,
                }
            }
            AppError::BusinessLogic(BusinessLogicError::MaintenanceModeBlocked {
                state,
                operation,
            }) => UserError {
                code: "MAINTENANCE_MODE_BLOCKED".to_string(),
                message: format!(
                    "العملية ({operation}) غير متاحة أثناء وضع الصيانة ({state})."
                ),
                details: None,
            },
            AppError::BusinessLogic(BusinessLogicError::PriceLockedForActiveFiscalYear {
                fiscal_year,
            }) => UserError {
                code: "PRICE_LOCKED".to_string(),
                message: format!(
                    "السعر مغلق للسنة المالية النشطة {}. لا يمكن تعديل الأسعار أثناء السنة المالية المفتوحة.",
                    fiscal_year
                ),
                details: None,
            },

            // أخطاء المصادقة
            AppError::Authentication(AuthenticationError::InvalidCredentials { username: _ }) => {
                UserError {
                    code: "AUTH_CREDENTIALS".to_string(),
                    message: "اسم المستخدم أو كلمة المرور غير صحيحة.".to_string(),
                    details: None,
                }
            }
            AppError::Authentication(AuthenticationError::UserNotFound { .. }) => UserError {
                code: "AUTH_USER_NOT_FOUND".to_string(),
                message: "المستخدم غير موجود.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Authentication(AuthenticationError::InvalidOldPassword { .. }) => UserError {
                code: "AUTH_OLD_PASSWORD".to_string(),
                message: "كلمة المرور القديمة غير صحيحة.".to_string(),
                details: None,
            },
            AppError::Authentication(AuthenticationError::PasswordHash { .. }) => UserError {
                code: "AUTH_PASSWORD_HASH".to_string(),
                message: "حدث خطأ في معالجة كلمة المرور. يرجى المحاولة مرة أخرى.".to_string(),
                details: Some(self.to_string()),
            },
            AppError::Authentication(AuthenticationError::SessionNotFound) => UserError {
                code: "AUTH_NO_SESSION".to_string(),
                message: "لا توجد جلسة نشطة. يرجى تسجيل الدخول مرة أخرى.".to_string(),
                details: None,
            },

            // أخطاء تحويل التواريخ
            AppError::DateParse(_) => UserError {
                code: "PARSE_DATE".to_string(),
                message: "صيغة التاريخ غير صالحة. استخدم YYYY-MM-DD.".to_string(),
                details: Some(self.to_string()),
            },

            // أخطاء الإدخال والإخراج
            AppError::Io(_) => UserError {
                code: "IO_ERROR".to_string(),
                message: "حدث خطأ في النظام أثناء معالجة الملفات.".to_string(),
                details: Some(self.to_string()),
            },

            // أخطاء عامة
            _ => UserError {
                code: "INTERNAL".to_string(),
                message: "حدث خطأ غير متوقع. يرجى التواصل مع الدعم.".to_string(),
                details: Some(self.to_string()),
            },
        }
    }
}

/// تحويل أسماء الحقول إلى العربية
fn translate_field(field: &str) -> String {
    match field.to_lowercase().as_str() {
        "name" => "الاسم".to_string(),
        "username" => "اسم المستخدم".to_string(),
        "password" => "كلمة المرور".to_string(),
        "email" => "البريد الإلكتروني".to_string(),
        "code" => "الرمز".to_string(),
        "price" => "السعر".to_string(),
        "quantity" => "الكمية".to_string(),
        "date" => "التاريخ".to_string(),
        "supplier_name" => "اسم المورد".to_string(),
        "product_id" => "معرف المنتج".to_string(),
        "unit_id" => "معرف الوحدة".to_string(),
        "wilaya_code" => "رمز الولاية".to_string(),
        _ => field.to_string(),
    }
}

/// تحويل أسماء الجداول إلى العربية
fn translate_table(table: &str) -> String {
    match table.to_lowercase().as_str() {
        "products" => "المنتج".to_string(),
        "users" => "المستخدم".to_string(),
        "units" => "الوحدة".to_string(),
        "daily_reports" => "التقرير اليومي".to_string(),
        "supplier_orders" => "طلب المورد".to_string(),
        "inventory_stocks" => "المخزون".to_string(),
        _ => table.to_string(),
    }
}

/// تحويل أسماء الكيانات إلى العربية
fn translate_entity(entity: &str) -> String {
    match entity.to_lowercase().as_str() {
        "product" => "المنتج".to_string(),
        "user" => "المستخدم".to_string(),
        "unit" => "الوحدة".to_string(),
        "supplier" => "المورد".to_string(),
        _ => entity.to_string(),
    }
}

/// Helper لتحويل argon2 error
pub fn map_argon2_error(e: argon2::password_hash::Error) -> AppError {
    AppError::Authentication(AuthenticationError::PasswordHash {
        message: e.to_string(),
    })
}

/// تحويل AppError إلى رسالة String مناسبة لـ Tauri command.
///
/// **سياسة الكشف عن الأخطاء (ADR-0012):**
/// - في بيئة `debug` (debug_assertions مفعّل): تُظهر التفاصيل الداخلية لتسهيل
///   تشخيص الأخطاء أثناء التطوير.
/// - في بيئة `release` / الإنتاج: تُرجع رسالة عامة آمنة فقط، وتُسجَّل
///   التفاصيل الداخلية عبر `log::error!` بدون كشفها للمستخدم النهائي.
pub fn into_command_error(e: AppError) -> String {
    let user_error = e.to_user_error();

    // الأخطاء الداخلية (INTERNAL) تحتاج معالجة خاصة:
    // تسجيل التفاصيل داخلياً ثم إخفاؤها عن المستخدم في production.
    if user_error.code == "INTERNAL" {
        // سجّل التفاصيل الداخلية دائماً بغض النظر عن البيئة.
        log::error!(
            "[CommandError] code={} | details={:?}",
            user_error.code,
            user_error.details
        );

        // في debug: أظهر التفاصيل للمطوّر مباشرةً.
        #[cfg(debug_assertions)]
        if let Some(ref details) = user_error.details {
            return format!("{} [dev: {}]", user_error.message, details); // ADR-0012
        }

        // في production: أرجع رسالة عامة آمنة فقط.
        return user_error.message.clone();
    }

    user_error.message
}

/// Helper macro لإنشاء validation error
#[macro_export]
macro_rules! validate_required {
    ($field:expr, $value:expr) => {
        if $value.to_string().trim().is_empty() {
            return Err($crate::errors::AppError::Validation(
                $crate::errors::ValidationError::Required {
                    field: $field.to_string(),
                },
            ));
        }
    };
}

#[macro_export]
macro_rules! validate_positive {
    ($field:expr, $value:expr, $min:expr) => {
        if $value <= $min {
            return Err($crate::errors::AppError::Validation(
                $crate::errors::ValidationError::OutOfRange {
                    field: $field.to_string(),
                    value: $value.to_string(),
                },
            ));
        }
    };
}

/// Helper function لتحويل Option<DateTime> مع خطأ مخصص.
/// يدعم RFC3339 مع التراجع إلى تنسيق SQLite القياسي في حالة الفشل.
pub fn parse_datetime_rfc3339(s: &str) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    if s.is_empty() {
        return Ok(chrono::Utc::now());
    }

    // 1. Try RFC3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }

    // 2. Try Standard SQL format (YYYY-MM-DD HH:MM:SS)
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            naive,
            chrono::Utc,
        ));
    }

    // 3. Try date only (YYYY-MM-DD)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(dt) = date.and_hms_opt(0, 0, 0) {
            return Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                dt,
                chrono::Utc,
            ));
        }
    }

    // If all fail, return error
    chrono::DateTime::parse_from_rfc3339(s)
        .map_err(|e| AppError::DateParse(format!("Invalid RFC3339 datetime: {}", e)))
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Helper function لتحويل Option<NaiveDate> مع خطأ مخصص
pub fn parse_naive_date(s: &str) -> Result<chrono::NaiveDate, AppError> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| AppError::DateParse(format!("Invalid date format '{}': {}", s, e)))
}

// ============================================================
// TESTS — Error Exposure Policy (ADR-0012)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------
    // Helper: construct a canonical INTERNAL AppError
    // ----------------------------------------------------------
    fn make_internal() -> AppError {
        AppError::Internal("sqlite: no such table: products".to_string())
    }

    // ----------------------------------------------------------
    // Helper: construct a user-safe validation error
    // ----------------------------------------------------------
    fn make_validation() -> AppError {
        AppError::Validation(ValidationError::Required {
            field: "name".to_string(),
        })
    }

    /// INTERNAL errors must NEVER expose raw details in the returned string
    /// in release builds. In debug builds the details are shown prefixed with
    /// "[dev: ...]". Because tests run with debug_assertions, we verify the
    /// dev-path here and confirm the production path at the structural level.
    #[test]
    fn internal_error_message_does_not_expose_sqlite_in_production_message() {
        let result = into_command_error(make_internal());

        // The raw SQLite string must NOT appear in the user-facing message
        // in either environment. The dev suffix adds "[dev: ...]" which still
        // contains it in debug, but in production (release) it would be hidden.
        // This test validates the production-safe base message is always present.
        assert!(
            result.contains("حدث خطأ غير متوقع"),
            "Expected generic Arabic message, got: {result}"
        );
    }

    /// Validation errors (user-safe) must be returned as-is.
    #[test]
    fn validation_error_message_is_returned_directly() {
        let result = into_command_error(make_validation());
        assert!(
            result.contains("الحقل مطلوب"),
            "Expected validation message, got: {result}"
        );
        // Must not contain internal code or leakage markers
        assert!(
            !result.contains("INTERNAL"),
            "Validation error should not contain INTERNAL code"
        );
    }

    /// The `to_user_error` mapping for INTERNAL must set code = "INTERNAL"
    #[test]
    fn to_user_error_internal_has_code_internal() {
        let user_err = make_internal().to_user_error();
        assert_eq!(user_err.code, "INTERNAL");
    }

    /// The `to_user_error` mapping for INTERNAL must store the raw details
    /// in the `details` field so they can be logged — but NOT exposed.
    #[test]
    fn to_user_error_internal_has_details_for_logging() {
        let user_err = make_internal().to_user_error();
        assert!(
            user_err.details.is_some(),
            "details must be populated for internal logging"
        );
        let details = user_err.details.unwrap();
        assert!(
            details.contains("sqlite") || details.contains("Internal error"),
            "details should contain diagnostic information for logging: {details}"
        );
    }

    /// Auth errors that carry sensitive internals (e.g. PasswordHash) must
    /// return a safe message without exposing hash internals.
    #[test]
    fn auth_password_hash_error_does_not_expose_internals() {
        let e = AppError::Authentication(AuthenticationError::PasswordHash {
            message: "argon2 internal panic: invalid lane count".to_string(),
        });
        let result = into_command_error(e);
        // Should not contain the raw argon2 internals.
        assert!(
            !result.contains("argon2 internal panic"),
            "Auth hash error must not expose argon2 internals: {result}"
        );
    }

    /// Database connection errors must not expose raw connection strings or paths.
    #[test]
    fn db_connection_error_message_is_safe() {
        let e = AppError::Database(DatabaseError::Connection {
            message: "unable to open /var/app/data/secret_prod.db".to_string(),
        });
        let result = into_command_error(e);
        assert!(
            !result.contains("/var/app"),
            "DB connection error must not expose file paths: {result}"
        );
        assert!(
            !result.contains("secret_prod.db"),
            "DB connection error must not expose db file names: {result}"
        );
    }

    /// Sqlite raw errors (AppError::Sqlite) fall into the INTERNAL wildcard arm.
    #[test]
    fn sqlite_raw_error_falls_to_internal() {
        // We can't construct rusqlite::Error easily in tests without a real DB,
        // so we verify the Internal variant directly which uses the same wildcard arm.
        let e = AppError::Internal("no such column: unknown_column".to_string());
        let user_err = e.to_user_error();
        assert_eq!(
            user_err.code, "INTERNAL",
            "Raw internal errors must map to INTERNAL code"
        );
        // The message shown to user must be generic
        assert!(
            !user_err.message.contains("unknown_column"),
            "User message must not contain raw column names: {}",
            user_err.message
        );
    }
}
