use crate::domain::units::{ProductUnitConfig, TvaClassification, UnitMeasure};
use crate::errors::{AppError, BusinessLogicError, ValidationError};
use crate::models::*;
use chrono::{NaiveDate, Utc};
use regex::Regex;
use std::sync::LazyLock;

static HTML_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("Invalid HTML regex pattern"));

static SPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("Invalid space regex pattern"));

static CODE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9]+$").expect("Invalid code regex pattern"));

/// Sanitize a string input by trimming and removing dangerous patterns
pub fn sanitize_string(input: &str) -> String {
    let mut result = input.trim().to_string();

    // Remove HTML tags to prevent XSS
    result = HTML_REGEX.replace_all(&result, "").to_string();

    // Normalize Arabic text for storage (safe normalization)
    result = normalize_arabic_for_storage(&result);

    result
}

/// Normalize Arabic text for safe storage
pub fn normalize_arabic_for_storage(input: &str) -> String {
    let mut result = input.to_string();

    // Remove tatweel (kashida) character U+0640
    result = result.replace('\u{0640}', "");

    // Replace multiple spaces with single space
    result = SPACE_REGEX.replace_all(&result, " ").to_string();

    result.trim().to_string()
}

/// Normalize Arabic text for fuzzy search/comparison
pub fn normalize_arabic_for_search(input: &str) -> String {
    let mut result = input.to_string();

    // Remove tatweel (kashida) character U+0640
    result = result.replace('\u{0640}', "");

    // Normalize different forms of alef
    result = result.replace(['أ', 'إ', 'آ'], "ا");

    // Normalize hamza on waw and ya
    result = result.replace('ؤ', "و").replace('ئ', "ي");

    // Normalize ta marbuta
    result = result.replace('ة', "ه");

    // Replace multiple spaces with single space
    result = SPACE_REGEX.replace_all(&result, " ").to_string();

    result.trim().to_string()
}

/// Validate that a string doesn't contain null bytes.
///
/// NOTE: Primitive regex blocklists for SQL injection are unreliable and provide a false sense of security.
/// This application relies on **parameterized queries** via `rusqlite` to prevent SQL injection.
/// We only check for null bytes here to prevent common string-truncation attacks in C-based libraries.
pub fn is_sql_safe(input: &str) -> bool {
    !input.contains('\0')
}

/// Validation result type
pub type ValidationResult = Result<(), AppError>;

/// Map a wire `Option<i32>` unit code to the closed-set [`UnitMeasure`], fail
/// closed on absence or an unknown code.
fn unit_from_option(field: &str, code: Option<i32>) -> Result<UnitMeasure, AppError> {
    let code = code.ok_or_else(|| {
        AppError::Validation(ValidationError::Required {
            field: field.to_string(),
        })
    })?;
    UnitMeasure::try_from(code).map_err(|_| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: field.to_string(),
            message: format!("كود وحدة غير صالح: {code}"),
        })
    })
}

/// Map a wire `Option<i32>` TVA code to the closed-set [`TvaClassification`],
/// fail closed on absence or an unknown code.
fn tva_from_option(code: Option<i32>) -> Result<TvaClassification, AppError> {
    let code = code.ok_or_else(|| {
        AppError::Validation(ValidationError::Required {
            field: "tva_classification".to_string(),
        })
    })?;
    TvaClassification::try_from(code).map_err(|_| {
        AppError::Validation(ValidationError::InvalidFormat {
            field: "tva_classification".to_string(),
            message: format!("تصنيف TVA غير صالح: {code}"),
        })
    })
}

/// Validate the product unit/TVA configuration (SEC-087) and return the typed
/// value set. **Single authoritative** unit/factor/TVA validation: the domain
/// validator, `ProductService`, and any future write path must source their
/// enforcement here so an invalid combination cannot reach a write path.
pub fn validate_product_units(
    purchase_unit: Option<i32>,
    consumption_unit: Option<i32>,
    conversion_factor: Option<i32>,
    tva_classification: Option<i32>,
) -> Result<ProductUnitConfig, AppError> {
    let purchase_unit = unit_from_option("purchase_unit", purchase_unit)?;
    let consumption_unit = unit_from_option("consumption_unit", consumption_unit)?;
    let tva_classification = tva_from_option(tva_classification)?;
    let conversion_factor = conversion_factor.ok_or_else(|| {
        AppError::Validation(ValidationError::Required {
            field: "conversion_factor".to_string(),
        })
    })?;
    if conversion_factor <= 0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "conversion_factor".to_string(),
            value: conversion_factor.to_string(),
        }));
    }
    // SEC-087 rule G: identical units require factor == 1.
    if purchase_unit == consumption_unit && conversion_factor != 1 {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "conversion_factor".to_string(),
            message: "عندما تتطابق وحدة الشراء ووحدة الاستهلاك يجب أن يكون معامل التحويل 1"
                .to_string(),
        }));
    }
    Ok(ProductUnitConfig {
        purchase_unit,
        consumption_unit,
        conversion_factor,
        tva_classification,
    })
}

/// Validate product creation request
pub fn validate_create_product_request(req: &CreateProductRequest, year: i32) -> ValidationResult {
    // Validate name
    let name = sanitize_string(&req.name);
    if name.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "name".to_string(),
        }));
    }
    if name.len() < 2 || name.len() > 100 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "name".to_string(),
            value: format!("الطول: {}", name.len()),
        }));
    }
    if !is_sql_safe(&name) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "name".to_string(),
            message: "اسم المنتج يحتوي على محتوى غير آمن".to_string(),
        }));
    }

    // Validate base_price
    if req.base_price <= 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidPrice {
            value: req.base_price,
        }));
    }
    if req.base_price > 1_000_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "base_price".to_string(),
            value: req.base_price.to_string(),
        }));
    }

    // Validate year
    if !(2020..=2100).contains(&year) {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "year".to_string(),
            value: year.to_string(),
        }));
    }

    // SEC-087: units, conversion factor and TVA classification are required and
    // must be a valid closed-set combination; fail closed so no invalid product
    // can be created.
    validate_product_units(
        req.purchase_unit,
        req.consumption_unit,
        req.conversion_factor,
        req.tva_classification,
    )?;

    Ok(())
}

/// Validate product update request
pub fn validate_update_product_request(req: &UpdateProductRequest) -> ValidationResult {
    // Validate ID
    if req.id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "id".to_string(),
        }));
    }

    // Validate name
    let name = sanitize_string(&req.name);
    if name.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "name".to_string(),
        }));
    }
    if name.len() < 2 || name.len() > 100 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "name".to_string(),
            value: format!("الطول: {}", name.len()),
        }));
    }
    if !is_sql_safe(&name) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "name".to_string(),
            message: "اسم المنتج يحتوي على محتوى غير آمن".to_string(),
        }));
    }

    // Validate base_price
    if req.base_price <= 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidPrice {
            value: req.base_price,
        }));
    }
    if req.base_price > 1_000_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "base_price".to_string(),
            value: req.base_price.to_string(),
        }));
    }

    Ok(())
}

fn validate_meal_section_input(section: &MealSectionInput) -> ValidationResult {
    let beneficiary_fields = [
        ("staff_24h_count", section.staff_24h_count),
        ("staff_8h_count", section.staff_8h_count),
        ("reservation_count", section.reservation_count),
        ("mission_count", section.mission_count),
        ("guest_count", section.guest_count),
    ];

    for (field, value) in beneficiary_fields {
        if !(0..=1000).contains(&value) {
            return Err(AppError::Validation(ValidationError::OutOfRange {
                field: field.to_string(),
                value: value.to_string(),
            }));
        }
    }

    let has_items = section.items.iter().any(|i| i.quantity > 0.0);
    let total = DailyReportMeal::compute_total_beneficiaries(
        section.staff_24h_count,
        section.staff_8h_count,
        section.reservation_count,
        section.mission_count,
        section.guest_count,
    );

    if has_items && total < 1 {
        return Err(AppError::Validation(ValidationError::Required {
            field: format!("beneficiaries_{}", section.meal_type.as_str()),
        }));
    }

    for item in &section.items {
        if item.quantity > 0.0 {
            validate_consumption_item(item)?;
        }
    }

    Ok(())
}

/// Validate full daily report input (one report, multiple meal sections)
pub fn validate_daily_report_input(input: &DailyReportInput) -> ValidationResult {
    let today = Utc::now().date_naive();
    let one_year_ago = today - chrono::Duration::days(365);

    if input.date > today {
        return Err(AppError::Validation(ValidationError::InvalidDate {
            value: "لا يمكن إنشاء تقرير لتاريخ في المستقبل".to_string(),
        }));
    }
    if input.date < one_year_ago {
        return Err(AppError::Validation(ValidationError::InvalidDate {
            value: "لا يمكن إنشاء تقرير لتاريخ أقدم من سنة".to_string(),
        }));
    }

    if input.meals.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "meals".to_string(),
        }));
    }

    let mut seen = std::collections::HashSet::new();
    let mut has_any_consumption = false;

    for section in &input.meals {
        if !seen.insert(section.meal_type) {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "meal_type".to_string(),
                message: "نوع الوجبة مكرر".to_string(),
            }));
        }
        validate_meal_section_input(section)?;

        let has_items = section.items.iter().any(|i| i.quantity > 0.0);
        let total = DailyReportMeal::compute_total_beneficiaries(
            section.staff_24h_count,
            section.staff_8h_count,
            section.reservation_count,
            section.mission_count,
            section.guest_count,
        );
        if has_items || total > 0 {
            has_any_consumption = true;
        }
    }

    if !has_any_consumption {
        return Err(AppError::Validation(ValidationError::Required {
            field: "consumption".to_string(),
        }));
    }

    Ok(())
}

pub fn validate_meal_consumption_input(input: &MealSectionInput) -> ValidationResult {
    validate_meal_section_input(input)
}

pub fn validate_daily_consumption_input(input: &DailyReportInput) -> ValidationResult {
    validate_daily_report_input(input)
}

/// Validate consumption item input
fn validate_consumption_item(item: &ConsumptionItemInput) -> ValidationResult {
    // Validate product_id
    if item.product_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "product_id".to_string(),
        }));
    }

    // Validate quantity
    if item.quantity <= 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidQuantity {
            value: item.quantity,
        }));
    }
    if item.quantity > 100_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "quantity".to_string(),
            value: item.quantity.to_string(),
        }));
    }

    Ok(())
}

/// Validate order creation request
pub fn validate_create_order_request(req: &CreateOrderRequest) -> ValidationResult {
    // Validate reference_number if present
    if let Some(ref reference) = req.reference_number {
        let reference = sanitize_string(reference);
        if !reference.is_empty() && !is_sql_safe(&reference) {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "reference_number".to_string(),
                message: "رقم المرجع يحتوي على محتوى غير آمن".to_string(),
            }));
        }
    }

    // Validate items
    if req.items.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "items".to_string(),
        }));
    }

    for item in &req.items {
        validate_order_item_input(item)?;
    }

    Ok(())
}

/// Validate draft order update (same rules as create + order id)
pub fn validate_update_order_request(req: &UpdateOrderRequest) -> ValidationResult {
    if req.id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "id".to_string(),
        }));
    }
    validate_create_order_request(&CreateOrderRequest {
        reference_number: req.reference_number.clone(),
        items: req.items.clone(),
    })
}

/// Validate order item input (product + quantity only; price is backend-resolved)
fn validate_order_item_input(item: &OrderItemInput) -> ValidationResult {
    // Validate product_id
    if item.product_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "product_id".to_string(),
        }));
    }

    // Validate quantity
    if item.quantity <= 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidQuantity {
            value: item.quantity,
        }));
    }
    if item.quantity > 100_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "quantity".to_string(),
            value: item.quantity.to_string(),
        }));
    }

    Ok(())
}

// ------------------------------------------------------------------
// Supplier / Contract / Fiscal Tax Policy validation
// (ADR-0055 / SEC-087-F)
// ------------------------------------------------------------------

/// Supplier name: trimmed, non-empty, SQL-safe, ≤200 chars.
pub fn validate_create_supplier_request(req: &CreateSupplierRequest) -> ValidationResult {
    let name = sanitize_string(&req.name);
    if name.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "name".to_string(),
        }));
    }
    if !is_sql_safe(&name) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "name".to_string(),
            message: "اسم المورد يحتوي على محتوى غير آمن".to_string(),
        }));
    }
    if name.len() > 200 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "name".to_string(),
            value: name.len().to_string(),
        }));
    }
    Ok(())
}

pub fn validate_update_supplier_request(req: &UpdateSupplierRequest) -> ValidationResult {
    if req.id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "id".to_string(),
        }));
    }
    validate_create_supplier_request(&CreateSupplierRequest {
        name: req.name.clone(),
        contact_info: req.contact_info.clone(),
    })
}

/// Contract reference and fiscal year range.
pub fn validate_create_contract_request(req: &CreateContractRequest) -> ValidationResult {
    let reference = sanitize_string(&req.contract_reference);
    if reference.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "contract_reference".to_string(),
        }));
    }
    if !is_sql_safe(&reference) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "contract_reference".to_string(),
            message: "مرجع العقد يحتوي على محتوى غير آمن".to_string(),
        }));
    }
    if reference.len() > 100 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "contract_reference".to_string(),
            value: reference.len().to_string(),
        }));
    }
    if req.unit_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "unit_id".to_string(),
        }));
    }
    if req.supplier_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "supplier_id".to_string(),
        }));
    }
    if req.fiscal_year < 2020 || req.fiscal_year > 2099 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "fiscal_year".to_string(),
            value: req.fiscal_year.to_string(),
        }));
    }
    Ok(())
}

pub fn validate_add_contract_product_request(req: &AddContractProductRequest) -> ValidationResult {
    if req.contract_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "contract_id".to_string(),
        }));
    }
    if req.product_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "product_id".to_string(),
        }));
    }
    if req.proposed_price_ht < 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidPrice {
            value: req.proposed_price_ht,
        }));
    }
    if let Some(price) = req.agreed_price_ht {
        if price < 0.0 {
            return Err(AppError::Validation(ValidationError::InvalidPrice {
                value: price,
            }));
        }
    }
    if req.contracted_quantity <= 0.0 || req.contracted_quantity > 1_000_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "contracted_quantity".to_string(),
            value: req.contracted_quantity.to_string(),
        }));
    }
    Ok(())
}

/// Freeze agreed HT price: contract must still be in `proposed` state
/// (enforced at repo level), price must be non-negative.
pub fn validate_set_agreed_price_ht_request(req: &SetAgreedPriceHtRequest) -> ValidationResult {
    if req.contract_product_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "contract_product_id".to_string(),
        }));
    }
    if req.agreed_price_ht < 0.0 {
        return Err(AppError::Validation(ValidationError::InvalidPrice {
            value: req.agreed_price_ht,
        }));
    }
    Ok(())
}

pub fn validate_release_contract_allocation_request(
    req: &ReleaseContractAllocationRequest,
) -> ValidationResult {
    if req.allocation_id.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "allocation_id".to_string(),
        }));
    }
    if req.released_quantity <= 0.0 || req.released_quantity > 1_000_000.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "released_quantity".to_string(),
            value: req.released_quantity.to_string(),
        }));
    }
    Ok(())
}

/// Fiscal-year TVA policy: one rate per FY, 0.0–100.0 inclusive.
pub fn validate_set_tax_policy_request(req: &SetTaxPolicyRequest) -> ValidationResult {
    if req.fiscal_year < 2020 || req.fiscal_year > 2099 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "fiscal_year".to_string(),
            value: req.fiscal_year.to_string(),
        }));
    }
    if req.tva_rate < 0.0 || req.tva_rate > 100.0 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "tva_rate".to_string(),
            value: req.tva_rate.to_string(),
        }));
    }
    Ok(())
}

/// Validate unit creation request
pub fn validate_create_unit_request(req: &CreateUnitRequest) -> ValidationResult {
    // Validate code (exactly 6 alphanumeric characters)
    let code = sanitize_string(&req.code);
    if code.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "code".to_string(),
        }));
    }
    if code.len() != 6 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "code".to_string(),
            value: format!("الطول: {} (يجب أن يكون 6 أحرف)", code.len()),
        }));
    }
    if !CODE_REGEX.is_match(&code) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "code".to_string(),
            message: "كود الوحدة يجب أن يكون 4-20 حرفاً أبجدياً رقمياً".to_string(),
        }));
    }

    // Validate name (3-100 characters)
    let name = sanitize_string(&req.name);
    if name.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "name".to_string(),
        }));
    }
    if name.len() < 3 || name.len() > 100 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "name".to_string(),
            value: format!("الطول: {}", name.len()),
        }));
    }
    if !is_sql_safe(&name) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "name".to_string(),
            message: "اسم الوحدة يحتوي على محتوى غير آمن".to_string(),
        }));
    }

    // ADR-0052: there is no caller-supplied username to validate — the
    // operator username is derived server-side as the canonical `user`.

    // Validate password (8+ characters, must contain uppercase, lowercase, and digit)
    if req.password.len() < 8 {
        return Err(AppError::Validation(ValidationError::WeakPassword {
            reason: "كلمة المرور يجب أن تكون 8 أحرف على الأقل".to_string(),
        }));
    }

    let has_uppercase = req.password.chars().any(|c| c.is_ascii_uppercase());
    let has_lowercase = req.password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = req.password.chars().any(|c| c.is_ascii_digit());

    if !has_uppercase || !has_lowercase || !has_digit {
        return Err(AppError::Validation(ValidationError::WeakPassword {
            reason: "كلمة المرور يجب أن تحتوي على حرف كبير وحرف صغير ورقم".to_string(),
        }));
    }

    Ok(())
}

/// Validate login request
pub fn validate_login_request(req: &LoginRequest) -> ValidationResult {
    let username = sanitize_string(&req.username);
    if username.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "username".to_string(),
        }));
    }
    if username.len() < 3 {
        return Err(AppError::Validation(ValidationError::InvalidUsername {
            reason: "اسم المستخدم يجب أن يكون 3 أحرف على الأقل".to_string(),
        }));
    }

    if req.password.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "password".to_string(),
        }));
    }

    Ok(())
}

/// Validate wilaya configuration
pub fn validate_configure_wilaya(wilaya_code: &str, wilaya_name: &str) -> ValidationResult {
    let code = sanitize_string(wilaya_code);
    if code.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "wilaya_code".to_string(),
        }));
    }
    // Wilaya code must be exactly 2 digits
    if code.len() != 2 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "wilaya_code".to_string(),
            message: "كود الولاية يجب أن يكون رقمين (مثال: 09)".to_string(),
        }));
    }

    let name = sanitize_string(wilaya_name);
    if name.is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "wilaya_name".to_string(),
        }));
    }
    // Check for SQL injection patterns
    if !is_sql_safe(&name) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "wilaya_name".to_string(),
            message: "اسم الولاية يحتوي على محتوى غير آمن".to_string(),
        }));
    }
    if name.len() < 2 || name.len() > 100 {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "wilaya_name".to_string(),
            value: format!("الطول: {}", name.len()),
        }));
    }

    Ok(())
}

/// Validate stock availability
pub fn validate_stock_availability(items: &[(String, f64, f64)]) -> ValidationResult {
    // items: Vec of (product_id, requested_qty, available_qty)
    for (product_id, requested, available) in items {
        if *requested > *available {
            return Err(AppError::BusinessLogic(
                BusinessLogicError::InsufficientStock(format!(
                    "Product {}: requested {}, available {}",
                    product_id, requested, available
                )),
            ));
        }
    }

    Ok(())
}

/// Business rule: Check if a daily report already exists for this date
pub fn check_duplicate_daily_report(
    existing_report: Option<&DailyReport>,
    date: NaiveDate,
) -> ValidationResult {
    if existing_report.is_some() {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::ReportAlreadyExists {
                date: date.to_string(),
            },
        ));
    }

    Ok(())
}

/// Business rule: Check if order is already confirmed
pub fn check_order_already_confirmed(order: &SupplierOrder) -> ValidationResult {
    check_order_is_editable(order)
}

/// Only draft orders may be edited or deleted
pub fn check_order_is_editable(order: &SupplierOrder) -> ValidationResult {
    if order.status != OrderStatus::Draft {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::OrderAlreadyConfirmed {
                order_id: order.id.clone(),
            },
        ));
    }
    Ok(())
}

/// Validate order can be confirmed (has items)
pub fn validate_order_can_be_confirmed(items: &[SupplierOrderItem]) -> ValidationResult {
    if items.is_empty() {
        return Err(AppError::BusinessLogic(
            BusinessLogicError::PriceCalculation {
                message: "لا يمكن تأكيد طلب فارغ".to_string(),
            },
        ));
    }

    Ok(())
}

/// Validate password change request
pub fn validate_change_password(new_password: &str) -> ValidationResult {
    if new_password.len() < 8 {
        return Err(AppError::Validation(ValidationError::WeakPassword {
            reason: "كلمة المرور يجب أن تكون 8 أحرف على الأقل".to_string(),
        }));
    }

    let has_uppercase = new_password.chars().any(|c| c.is_ascii_uppercase());
    let has_lowercase = new_password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = new_password.chars().any(|c| c.is_ascii_digit());

    if !has_uppercase || !has_lowercase || !has_digit {
        return Err(AppError::Validation(ValidationError::WeakPassword {
            reason: "كلمة المرور يجب أن تحتوي على حرف كبير وحرف صغير ورقم".to_string(),
        }));
    }

    Ok(())
}

/// التحقق من صحة مسار الملف للحماية من Path Traversal
///
/// # Arguments
/// * `path` - مسار الملف المراد التحقق منه
/// * `allowed_extensions` - قائمة بالامتدادات المسموح بها (مثل ["sync", "unit", "bak"])
///
/// # Returns
/// * `Ok(())` إذا كان المسار صالحاً وآمناً
///
/// # Errors
/// * إذا كان المسار يحتوي على محاولات path traversal (../)
/// * إذا كان الامتداد غير مسموح به
/// * إذا كان المسار فارغاً أو غير صالح
pub fn validate_file_path(path: &str, allowed_extensions: &[&str]) -> ValidationResult {
    // التحقق من أن المسار غير فارغ
    if path.trim().is_empty() {
        return Err(AppError::Validation(ValidationError::Required {
            field: "file_path".to_string(),
        }));
    }

    // 1. Block UNC paths and Windows Device paths (\\ or //)
    let trimmed = path.trim();
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "file_path".to_string(),
            message: "لا يسمح بمسارات الشبكة (UNC)".to_string(),
        }));
    }

    // 2. التحقق من عدم وجود محاولات path traversal
    let dangerous_patterns = ["../", "..\\", "..", "%2e%2e%2f", "%252e%252e%252f"];
    for pattern in &dangerous_patterns {
        if path.contains(pattern) {
            return Err(AppError::Validation(ValidationError::InvalidFormat {
                field: "file_path".to_string(),
                message: format!("مسار الملف يحتوي على نمط خطير: {}", pattern),
            }));
        }
    }

    // 3. التحقق من الامتداد
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !allowed_extensions.contains(&extension.as_str()) {
        return Err(AppError::Validation(ValidationError::InvalidFormat {
            field: "file_path".to_string(),
            message: format!(
                "امتداد الملف غير مسموح به. الامتدادات المسموحة: {:?}",
                allowed_extensions
            ),
        }));
    }

    // 4. Harden path using canonicalize (parent directory if file doesn't exist yet)
    let path_obj = std::path::Path::new(path);
    let (target_to_canonicalize, _is_new_file) = if path_obj.exists() {
        (path_obj, false)
    } else {
        (
            path_obj
                .parent()
                .unwrap_or_else(|| std::path::Path::new(".")),
            true,
        )
    };

    match std::fs::canonicalize(target_to_canonicalize) {
        Ok(canonical_path) => {
            let canonical_str = canonical_path.to_string_lossy();

            // Check for symlinks in the existing path segment (by checking metadata of canonical path, though canonicalize resolves them)
            // If the user provided a symlink, canonicalize resolves it, but we may want to ensure it doesn't escape allowed roots.
            // For desktop apps, we just ensure no UNC/Network paths were resolved.
            if canonical_str.starts_with("\\\\?\\UNC\\") {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "file_path".to_string(),
                    message: "تم اكتشاف مسار شبكة غير مسموح به بعد المعالجة".to_string(),
                }));
            }
        }
        Err(_e) => {
            // Ignore NotFound if we are just checking a relative/test path in tests
            #[cfg(not(test))]
            if _e.kind() != std::io::ErrorKind::NotFound {
                return Err(AppError::Validation(ValidationError::InvalidFormat {
                    field: "file_path".to_string(),
                    message: "مسار غير صالح أو غير مسموح بالوصول إليه".to_string(),
                }));
            }
        }
    }

    Ok(())
}

/// Year/month bounds shared by report exports and similar adapters.
pub fn validate_calendar_month(year: i32, month: i32) -> ValidationResult {
    if !(2020..=2100).contains(&year) {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "year".to_string(),
            value: year.to_string(),
        }));
    }
    if !(1..=12).contains(&month) {
        return Err(AppError::Validation(ValidationError::OutOfRange {
            field: "month".to_string(),
            value: month.to_string(),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_string_trims_whitespace() {
        let input = "  test value  ";
        let result = sanitize_string(input);
        assert_eq!(result, "test value");
    }

    #[test]
    fn test_sanitize_string_removes_html() {
        let input = "<script>alert('xss')</script>test";
        let result = sanitize_string(input);
        assert!(!result.contains("<script>"));
        assert!(result.contains("test"));
    }

    #[test]
    fn test_normalize_arabic_for_storage() {
        let input = "مـنـتـج   أول";
        let result = normalize_arabic_for_storage(input);
        assert_eq!(result, "منتج أول"); // Alef with hamza is preserved

        assert_eq!(normalize_arabic_for_storage("فاكهة"), "فاكهة");
        assert_eq!(normalize_arabic_for_storage("مسؤول"), "مسؤول");
        assert_eq!(normalize_arabic_for_storage("مائدة"), "مائدة");
    }

    #[test]
    fn test_normalize_arabic_for_search() {
        assert_eq!(normalize_arabic_for_search("مـنـتـج   أول"), "منتج اول");
        assert_eq!(normalize_arabic_for_search("فاكهة"), "فاكهه");
        assert_eq!(normalize_arabic_for_search("مسؤول"), "مسوول");
        assert_eq!(normalize_arabic_for_search("مائدة"), "مايده");
    }

    #[test]
    fn test_is_sql_safe_detects_null_bytes() {
        assert!(!is_sql_safe("test\0safe"));
        assert!(is_sql_safe("'; DROP TABLE users; --")); // Now considered safe by this function, as we rely on parameterization
        assert!(is_sql_safe("منتج طبيعي"));
    }

    #[test]
    fn test_validate_create_product_request_success() {
        let req = CreateProductRequest {
            name: "منتج صالح".to_string(),
            base_price: 100.0,
            purchase_unit: Some(2),
            consumption_unit: Some(1),
            conversion_factor: Some(2),
            tva_classification: Some(2),
        };
        assert!(validate_create_product_request(&req, 2024).is_ok());
    }

    #[test]
    fn test_validate_create_product_request_empty_name() {
        let req = CreateProductRequest {
            name: "".to_string(),
            base_price: 100.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(2),
        };
        let result = validate_create_product_request(&req, 2024);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_create_product_request_negative_price() {
        let req = CreateProductRequest {
            name: "منتج".to_string(),
            base_price: -10.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(2),
        };
        let result = validate_create_product_request(&req, 2024);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_create_product_request_unknown_tva_fails_closed() {
        // SEC-087: TVA classification is again a product property; an unknown
        // code must fail closed (never default, never fall back).
        let req = CreateProductRequest {
            name: "منتج".to_string(),
            base_price: 100.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(99),
        };
        let result = validate_create_product_request(&req, 2024);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_product_units_fail_closed_invariants() {
        // Missing unit codes are rejected.
        assert!(validate_product_units(None, Some(1), Some(1), Some(2)).is_err());
        // Unknown unit codes are rejected.
        assert!(validate_product_units(Some(0), Some(1), Some(1), Some(2)).is_err());
        // Factor 0 / negative is rejected.
        assert!(validate_product_units(Some(1), Some(1), Some(0), Some(2)).is_err());
        assert!(validate_product_units(Some(1), Some(1), Some(-1), Some(2)).is_err());
        // Identical units require factor == 1 (SEC-087 rule G).
        assert!(validate_product_units(Some(1), Some(1), Some(2), Some(2)).is_err());
        // Distinct units with a positive factor pass.
        assert!(validate_product_units(Some(2), Some(1), Some(2), Some(2)).is_ok());
    }

    #[test]
    fn test_validate_create_unit_request_success() {
        let req = CreateUnitRequest {
            code: "UNIT01".to_string(),
            name: "وحدة الاختبار".to_string(),
            password: "Test1234".to_string(),
        };
        assert!(validate_create_unit_request(&req).is_ok());
    }

    #[test]
    fn test_validate_create_unit_request_invalid_code() {
        let req = CreateUnitRequest {
            code: "U1".to_string(),
            name: "وحدة".to_string(),
            password: "Test1234".to_string(),
        };
        let result = validate_create_unit_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_create_unit_request_weak_password() {
        let req = CreateUnitRequest {
            code: "UNIT01".to_string(),
            name: "وحدة".to_string(),
            password: "weak".to_string(),
        };
        let result = validate_create_unit_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_create_unit_request_password_no_uppercase() {
        let req = CreateUnitRequest {
            code: "UNIT01".to_string(),
            name: "وحدة".to_string(),
            password: "test1234".to_string(),
        };
        let result = validate_create_unit_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_login_request_success() {
        let req = LoginRequest {
            username: "admin".to_string(),
            password: "password".to_string(),
        };
        assert!(validate_login_request(&req).is_ok());
    }

    #[test]
    fn test_validate_login_request_empty_username() {
        let req = LoginRequest {
            username: "".to_string(),
            password: "password".to_string(),
        };
        let result = validate_login_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_configure_wilaya_success() {
        assert!(validate_configure_wilaya("09", "Blida").is_ok());
    }

    #[test]
    fn test_validate_configure_wilaya_empty_code() {
        let result = validate_configure_wilaya("", "Blida");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_path_success() {
        assert!(validate_file_path("data/export.sync", &["sync"]).is_ok());
        assert!(validate_file_path("data/file.unit", &["sync", "unit"]).is_ok());
        // Absolute paths should now be allowed for desktop UX
        #[cfg(windows)]
        assert!(validate_file_path("C:\\Users\\test\\export.sync", &["sync"]).is_ok());
        #[cfg(not(windows))]
        assert!(validate_file_path("/home/test/export.sync", &["sync"]).is_ok());
    }

    #[test]
    fn test_validate_file_path_traversal() {
        assert!(validate_file_path("../etc/passwd", &["sync"]).is_err());
        assert!(validate_file_path("..\\windows\\system32", &["sync"]).is_err());
        assert!(validate_file_path("data/../../../etc/passwd", &["sync"]).is_err());
    }

    #[test]
    fn test_validate_file_path_invalid_extension() {
        assert!(validate_file_path("data/file.exe", &["sync", "unit"]).is_err());
        assert!(validate_file_path("data/file", &["sync"]).is_err());
    }

    #[test]
    fn test_validate_file_path_empty() {
        assert!(validate_file_path("", &["sync"]).is_err());
        assert!(validate_file_path("   ", &["sync"]).is_err());
    }

    #[test]
    fn test_validate_create_product_request_extreme_values() {
        // قيم سالبة كبيرة
        let req = CreateProductRequest {
            name: "منتج".to_string(),
            base_price: -1_000_000.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(2),
        };
        let result = validate_create_product_request(&req, 2024);
        assert!(result.is_err());

        // قيم موجبة كبيرة جداً
        let req = CreateProductRequest {
            name: "منتج".to_string(),
            base_price: 10_000_000.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(2),
        };
        let result = validate_create_product_request(&req, 2024);
        assert!(result.is_err());

        // سعر معقول ضمن الحدود
        let req = CreateProductRequest {
            name: "منتج".to_string(),
            base_price: 100.0,
            purchase_unit: Some(1),
            consumption_unit: Some(1),
            conversion_factor: Some(1),
            tva_classification: Some(2),
        };
        assert!(validate_create_product_request(&req, 2024).is_ok());
    }

    #[test]
    fn test_validate_daily_consumption_input_edge_cases() {
        let input = MealSectionInput {
            meal_type: MealType::Breakfast,
            staff_24h_count: 1_000_000,
            staff_8h_count: 0,
            reservation_count: 0,
            mission_count: 0,
            guest_count: 0,
            items: vec![ConsumptionItemInput {
                product_id: "test".to_string(),
                quantity: 1.0,
            }],
        };
        let result = validate_meal_section_input(&input);
        assert!(result.is_err());

        // كمية استهلاك سالبة - هذا الاختبار للتوثيق فقط
        // Note: validate_daily_consumption_input doesn't check negative quantities
        // This test documents the current behavior
    }

    #[test]
    fn test_is_sql_safe_unicode_and_special_chars() {
        // أحرف عربية ويونيكود
        assert!(is_sql_safe("منتج طبيعي 100%"));
        assert!(is_sql_safe("اسم المنتج (فئة أ)"));
        assert!(is_sql_safe("سعر ١٠٠ دينار"));

        // محاولات حقن مشفرة - is_sql_safe لا يفك تشفير URL
        // لكن هذه ستتم معالجتها بواسطة sanitize_string قبل التحقق
        assert!(is_sql_safe("test.txt"));
        assert!(is_sql_safe("path/to/file.sync"));
    }
}
