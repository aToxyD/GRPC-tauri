//! Database Integration Tests
use chrono::{Datelike, Utc};
use grpc_lib::db::ConnectionFactory;
use grpc_lib::db::Database;
use grpc_lib::models::*;
use grpc_lib::repositories::{
    InventoryRepository, ProductRepository, ReportRepository, StockMovementRepository,
    UnitRepository,
};
use uuid::Uuid;

// Helper: إعداد قاعدة بيانات مع وحدة
pub fn setup_test_db_with_unit() -> (Database, String) {
    let db = ConnectionFactory::new_for_test().unwrap();
    let executor = db.executor();
    let now = Utc::now().to_rfc3339();

    // system user is automatically created by the initial migration

    // إنشاء وحدة اختبار
    let unit_id = Uuid::new_v4().to_string();
    let unit_repo = UnitRepository::new(executor);
    unit_repo
        .upsert_raw_unit(&unit_id, "TEST01", "Test Unit", "01", &now)
        .unwrap();

    executor.execute(
        "INSERT OR IGNORE INTO fiscal_year_status (year, status, opened_at) VALUES (2024, 'open', ?1)",
        rusqlite::params![now],
    ).unwrap();

    (db, unit_id)
}

// Helper: إنشاء وحدة ثانية
pub fn create_second_test_unit(db: &Database) -> String {
    let unit_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let unit_repo = UnitRepository::new(db.executor());
    unit_repo
        .upsert_raw_unit(&unit_id, "TEST02", "Second Test Unit", "01", &now)
        .unwrap();
    unit_id
}

// Helper: إنشاء منتج اختبار
// Helper: إنشاء منتج اختبار
pub fn create_test_product(db: &Database, id: &str, name: &str) -> String {
    let now = Utc::now().to_rfc3339();
    let executor = db.executor();

    let product_repo = ProductRepository::new(executor);
    product_repo
        .insert_raw_product(
            &grpc_lib::models::Product {
                id: id.to_string(),
                name: name.to_string(),
                base_price: 100.0,
                year: 2024,
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();

    let inventory_repo = InventoryRepository::new(executor);
    let stock_id = Uuid::new_v4().to_string();
    inventory_repo
        .create_initial_stock_for_product(&stock_id, id, None, &now)
        .unwrap();

    id.to_string()
}

// Helper: إنشاء تقرير يومي (رأس + وجبة فطور للاختبارات)
pub fn create_test_daily_report(db: &Database, unit_id: &str, date: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let meal_id = format!("{id}-breakfast");
    let now = Utc::now().to_rfc3339();
    let naive_date: chrono::NaiveDate = date.parse().unwrap();
    let report_repo = ReportRepository::new(db.executor());

    report_repo
        .insert_raw_daily_report(
            &id,
            &grpc_lib::models::DailyReport {
                id: id.clone(),
                date: naive_date,
                unit_id: Some(unit_id.to_string()),
                total_daily_cost: 1000.0,
                total_daily_average: 66.67,
                total_daily_beneficiaries: 15,
                fiscal_year: naive_date.year(),
                created_at: Utc::now(),
            },
            &now,
        )
        .unwrap();

    report_repo
        .insert_raw_meal(&grpc_lib::models::DailyReportMeal {
            id: meal_id,
            daily_report_id: id.clone(),
            meal_type: grpc_lib::models::MealType::Breakfast,
            staff_24h_count: 5,
            staff_8h_count: 3,
            reservation_count: 1,
            mission_count: 1,
            guest_count: 5,
            total_beneficiaries: 15,
            total_meal_cost: 1000.0,
            meal_average: 66.67,
        })
        .unwrap();

    id
}

pub fn test_breakfast_meal_id(report_id: &str) -> String {
    format!("{report_id}-breakfast")
}

// Helper: إنشاء حركة مخزون مباشرة (للاختبارات)
#[allow(clippy::too_many_arguments)]
pub fn create_stock_movement(
    db: &Database,
    product_id: &str,
    movement_type: StockMovementType,
    quantity: f64,
    balance_before: f64,
    balance_after: f64,
    reference_type: Option<&str>,
    reference_id: Option<&str>,
    timestamp: &str,
    unit_id: Option<&str>,
) -> String {
    let id = Uuid::new_v4().to_string();
    let executor = db.executor();
    let movement_repo = StockMovementRepository::new(executor);

    movement_repo
        .insert_raw_stock_movement(&grpc_lib::models::StockMovement {
            id: id.clone(),
            product_id: product_id.to_string(),
            product_name: Some("Test Product".to_string()),
            movement_type,
            quantity,
            balance_before,
            balance_after,
            reference_type: reference_type.map(|s| s.to_string()),
            reference_id: reference_id.map(|s| s.to_string()),
            notes: None,
            timestamp: timestamp.to_string(),
            user_id: "system".to_string(),
            username: "test_user".to_string(),
            unit_id: unit_id.map(|s| s.to_string()),
            fiscal_year: Some(2024),
            unit_cost: None,
        })
        .unwrap();

    // تحديث المخزون عبر المستودع
    let inventory_repo = InventoryRepository::new(executor);
    inventory_repo
        .update_stock(product_id, balance_after)
        .unwrap();

    id
}

#[test]
fn test_opening_is_last_balance_before_month_unit_scoped() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod1", "Test Product");

    // === شهر يناير: بناء الرصيد ===
    let dr_jan = create_test_daily_report(&db, &unit_id, "2024-01-15");

    // إضافة عنصر استهلاك للتقرير
    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_jan),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 10.0,
            unit_price: 100.0,
            total_cost: 1000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // أولاً: دخول مخزون في يناير (balance_after = 100)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_jan),
        "2024-01-10T10:00:00Z",
        Some(&unit_id),
    );

    // ثانياً: خروج (استهلاك) في يناير
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        10.0,
        100.0,
        90.0,
        Some("Consumption"),
        Some(&dr_jan),
        "2024-01-15T12:00:00Z",
        Some(&unit_id),
    );

    // === شهر فبراير: نحتاج خروج للكشف عن المنتج ===
    let dr_feb = create_test_daily_report(&db, &unit_id, "2024-02-10");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_feb),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 5.0,
            unit_price: 100.0,
            total_cost: 500.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // خروج في فبراير للكشف عن المنتج في snapshot فبراير
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        5.0,
        90.0,
        85.0,
        Some("Consumption"),
        Some(&dr_feb),
        "2024-02-10T12:00:00Z",
        Some(&unit_id),
    );

    // حساب snapshot لفبراير
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 2, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 2)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // Opening يجب أن يكون 90 (آخر balance_after من يناير)
    assert!(
        (item.opening_stock - 90.0).abs() < 0.001,
        "Opening should be 90 from January's last balance (unit scoped), got {}",
        item.opening_stock
    );
}

#[test]
fn test_opening_correct_when_in_precedes_first_out_in_month() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod2", "Test Product 2");

    // تقرير فبراير
    let dr_feb = create_test_daily_report(&db, &unit_id, "2024-02-10");

    // إضافة عنصر استهلاك للتقرير
    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_feb),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 20.0,
            unit_price: 100.0,
            total_cost: 2000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // دخول في فبراير قبل أي خروج (للتأكد أن Opening يأخذ من قبل الشهر فقط)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        50.0,
        0.0,
        50.0,
        Some("Order"),
        Some(&dr_feb),
        "2024-02-10T10:00:00Z",
        Some(&unit_id),
    );

    // خروج في فبراير (للكشف عن المنتج)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        20.0,
        50.0,
        30.0,
        Some("Consumption"),
        Some(&dr_feb),
        "2024-02-15T12:00:00Z",
        Some(&unit_id),
    );

    // حساب snapshot لمارس
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 2, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 2)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // Opening يجب أن يكون 0 لأنه لا يوجد رصيد قبل فبراير
    assert!(
        item.opening_stock == 0.0,
        "Opening should be 0 when no prior balance, got {}",
        item.opening_stock
    );
    assert!(item.total_in == 50.0, "Total IN should be 50");
    assert!(item.total_out == 20.0, "Total OUT should be 20");
}

#[test]
fn test_new_product_first_month_no_false_anomaly() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod3", "New Product");

    // تقرير مارس
    let dr_mar = create_test_daily_report(&db, &unit_id, "2024-03-10");

    // إضافة عنصر استهلاك للتقرير
    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_mar),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 30.0,
            unit_price: 100.0,
            total_cost: 3000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // منتج جديد: دخول ثم خروج في نفس الشهر
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_mar),
        "2024-03-10T10:00:00Z",
        Some(&unit_id),
    );

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        30.0,
        100.0,
        70.0,
        Some("Consumption"),
        Some(&dr_mar),
        "2024-03-15T12:00:00Z",
        Some(&unit_id),
    );

    // حساب snapshot لمارس
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 3, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 3)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    assert!(
        item.opening_stock == 0.0,
        "Opening should be 0 for new product"
    );
    assert!(item.total_in == 100.0, "Total IN should be 100");
    assert!(item.total_out == 30.0, "Total OUT should be 30");
    assert!(
        !item.has_balance_anomaly,
        "New product first month should NOT have balance anomaly, even with consumption"
    );
}

#[test]
fn test_product_with_consumption_but_no_prior_stock_triggers_anomaly() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod4", "Suspicious Product");

    // تقرير أبريل
    let dr_apr = create_test_daily_report(&db, &unit_id, "2024-04-10");

    // إضافة عنصر استهلاك للتقرير
    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_apr),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 10.0,
            unit_price: 100.0,
            total_cost: 1000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // منتج جديد: دخول قليل ثم خروج
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        10.0,
        0.0,
        10.0,
        Some("Order"),
        Some(&dr_apr),
        "2024-04-10T10:00:00Z",
        Some(&unit_id),
    );

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        10.0,
        10.0,
        0.0,
        Some("Consumption"),
        Some(&dr_apr),
        "2024-04-15T12:00:00Z",
        Some(&unit_id),
    );

    // حساب snapshot لأبريل
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 4, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 4)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    assert!(item.opening_stock == 0.0, "Opening should be 0");
    assert!(item.total_in == 10.0, "Total IN should be 10");
    assert!(item.total_out == 10.0, "Total OUT should be 10");

    // لا شذوذ لأن total_in > 0 (منتج جديد)
    assert!(
        !item.has_balance_anomaly,
        "Product with IN > 0 should NOT trigger anomaly (new product case)"
    );
}

#[test]
fn test_total_in_includes_order_without_consumption_link() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_in_global", "Global IN Test");

    // تقرير يناير - consumption مطلوب للكشف عن المنتج
    let dr_jan = create_test_daily_report(&db, &unit_id, "2024-01-15");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_jan),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 10.0,
            unit_price: 100.0,
            total_cost: 1000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // دخول مخزون (IN) - global، ليس مرتبطًا بconsumption مباشرة
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_jan),
        "2024-01-10T10:00:00Z",
        Some(&unit_id),
    );

    // خروج (استهلاك)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        10.0,
        100.0,
        90.0,
        Some("Consumption"),
        Some(&dr_jan),
        "2024-01-15T12:00:00Z",
        Some(&unit_id),
    );

    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 1, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 1)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    assert_eq!(
        item.total_in, 100.0,
        "Total IN should include all IN movements globally"
    );
    assert_eq!(item.total_out, 10.0, "Total OUT should be unit-scoped");
}

#[test]
fn test_property_computed_closing_never_negative() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_no_neg", "No Negative Closing");

    // سيناريو: opening=0, in=10, out=20 (يؤدي إلى closing سالب نظرياً)
    let dr_jan = create_test_daily_report(&db, &unit_id, "2024-01-15");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_jan),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 20.0,
            unit_price: 100.0,
            total_cost: 2000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // دخول 10 فقط
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        10.0,
        0.0,
        10.0,
        Some("Order"),
        Some(&dr_jan),
        "2024-01-10T10:00:00Z",
        Some(&unit_id),
    );

    // خروج 20 (أكثر من المدخل)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        20.0,
        10.0,
        0.0, // balance_after يعكس 0 (لا يمكن أن يكون سالباً)
        Some("Consumption"),
        Some(&dr_jan),
        "2024-01-15T12:00:00Z",
        Some(&unit_id),
    );

    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 1, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 1)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // computed_closing = max(0, opening + in - out) = max(0, 0 + 10 - 20) = 0
    assert!(
        item.computed_closing >= 0.0,
        "Computed closing should never be negative, got {}",
        item.computed_closing
    );
}

#[test]
fn test_computed_closing_exact_fractional_arithmetic() {
    // ADR-0048: computed_closing is exact Quantity reconciliation:
    // opening + IN - OUT in scale-3 units, floored at zero.
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_frac", "Fractional Test");

    let dr_feb = create_test_daily_report(&db, &unit_id, "2024-02-15");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_feb),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 0.5,
            unit_price: 100.0,
            total_cost: 50.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // دخول 1.234 and خروج 0.500 في فبراير → closing = 1.234 - 0.500 = 0.734
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        1.234,
        0.0,
        1.234,
        Some("Order"),
        Some(&dr_feb),
        "2024-02-10T10:00:00Z",
        Some(&unit_id),
    );
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        0.5,
        1.234,
        0.734,
        Some("Consumption"),
        Some(&dr_feb),
        "2024-02-15T12:00:00Z",
        Some(&unit_id),
    );

    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 2, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 2)
        .unwrap()
        .unwrap();
    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    assert_eq!(item.total_in, 1.234);
    assert_eq!(item.total_out, 0.5);
    assert_eq!(
        item.computed_closing, 0.734,
        "Closing must be exact scale-3 arithmetic (1.234 - 0.500), got {}",
        item.computed_closing
    );
}

#[test]
fn test_snapshot_first_month_detection_is_exact_quantity() {
    // ADR-0048 (Target A): first-month detection is an exact Quantity
    // zero/inflow test. Opening exactly 0.000 with inflow > 0 is first-month;
    // opening exactly 0.001 (> 0) is not. No epsilon.
    let (db, unit_id) = setup_test_db_with_unit();
    let prod_first = create_test_product(&db, "prod_fm", "First Month");
    let prod_nfm = create_test_product(&db, "prod_nfm", "Not First Month");

    // prod_nfm gets an exact 0.001 opening for February via a January move;
    // prod_first has no prior movements → opening exactly 0.000.
    create_stock_movement(
        &db,
        &prod_nfm,
        StockMovementType::In,
        0.001,
        0.0,
        0.001,
        Some("Order"),
        None,
        "2024-01-20T08:00:00Z",
        Some(&unit_id),
    );

    let dr = create_test_daily_report(&db, &unit_id, "2024-02-15");

    for pid in [prod_first.clone(), prod_nfm.clone()] {
        create_stock_movement(
            &db,
            &pid,
            StockMovementType::In,
            0.5,
            0.0,
            0.5,
            Some("Order"),
            Some(&dr),
            "2024-02-05T08:00:00Z",
            Some(&unit_id),
        );
        create_stock_movement(
            &db,
            &pid,
            StockMovementType::Out,
            0.5,
            0.5,
            0.5,
            Some("Consumption"),
            Some(&dr),
            "2024-02-15T12:00:00Z",
            Some(&unit_id),
        );
    }

    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 2, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 2)
        .unwrap()
        .unwrap();

    let fm = view
        .items
        .iter()
        .find(|i| i.product_id == prod_first)
        .unwrap();
    let nfm = view
        .items
        .iter()
        .find(|i| i.product_id == prod_nfm)
        .unwrap();

    // prod_first: opening 0.000, inflow 0.500 → first month → reported 0.5 vs
    // computed 0.000 yields variance 0.5 but the anomaly is suppressed.
    assert!(
        !fm.has_balance_anomaly,
        "exact-zero opening must be detected as first month"
    );
    // prod_nfm: opening 0.001 (> 0) → NOT first month → variance 0.499 > 0.01
    // is flagged exactly.
    assert!(
        nfm.has_balance_anomaly,
        "nonzero opening must not be treated as first month"
    );
}

#[test]
fn test_product_base_price_change_detection_is_exact_money() {
    // ADR-0048: `base_price` change detection compares exact boundary-scaled
    // Money values (no float epsilon). Identical price → no fiscal lock check
    // (update succeeds in an open year); different price → PriceLocked error.
    use grpc_lib::application::services::ProductService;
    use grpc_lib::errors::BusinessLogicError;

    let (db, unit_id) = setup_test_db_with_unit();
    let _ = unit_id;
    let product_id = create_test_product(&db, "prod_price", "Price Test");

    let service = ProductService::new(db.executor());

    // Same price → no change → update allowed even with fiscal year open.
    service
        .update_product(&grpc_lib::models::UpdateProductRequest {
            id: product_id.clone(),
            name: "Price Test".into(),
            base_price: 100.0,
        })
        .expect("identical base_price must not be treated as a change");

    // Different price → change detected → open fiscal year blocks it.
    let err = service
        .update_product(&grpc_lib::models::UpdateProductRequest {
            id: product_id.clone(),
            name: "Price Test".into(),
            base_price: 101.0,
        })
        .expect_err("price change in an open fiscal year must be rejected");
    assert!(matches!(
        err,
        grpc_lib::errors::AppError::BusinessLogic(
            BusinessLogicError::PriceLockedForActiveFiscalYear { .. }
        )
    ));
}

#[test]
fn test_property_formula_is_deterministic() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_determ", "Deterministic Test");

    // بناء سيناريو ثابت
    let dr_jan = create_test_daily_report(&db, &unit_id, "2024-01-15");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_jan),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 30.0,
            unit_price: 100.0,
            total_cost: 3000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_jan),
        "2024-01-10T10:00:00Z",
        Some(&unit_id),
    );

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        30.0,
        100.0,
        70.0,
        Some("Consumption"),
        Some(&dr_jan),
        "2024-01-15T12:00:00Z",
        Some(&unit_id),
    );

    // حساب snapshot مرتين
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 1, false)
        .unwrap();
    let view1 = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 1)
        .unwrap()
        .unwrap();
    let item1 = view1
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // force recompute
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_id, 2024, 1, true)
        .unwrap();
    let view2 = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_id, 2024, 1)
        .unwrap()
        .unwrap();
    let item2 = view2
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // يجب أن تكون النتائج متطابقة
    assert_eq!(
        item1.opening_stock, item2.opening_stock,
        "Opening should be deterministic"
    );
    assert_eq!(
        item1.total_in, item2.total_in,
        "Total IN should be deterministic"
    );
    assert_eq!(
        item1.total_out, item2.total_out,
        "Total OUT should be deterministic"
    );
    assert_eq!(
        item1.computed_closing, item2.computed_closing,
        "Computed closing should be deterministic"
    );

    // التحقق من المعادلة: closing = opening + in - out
    let expected_closing = item1.opening_stock + item1.total_in - item1.total_out;
    assert!(
        (item1.computed_closing - expected_closing.max(0.0)).abs() < 0.001,
        "Formula should be: closing = max(0, opening + in - out)"
    );
}

#[test]
fn test_opening_does_not_leak_other_units_balance() {
    let (db, unit_a) = setup_test_db_with_unit();
    let unit_b = create_second_test_unit(&db);

    let product_id = create_test_product(&db, "prod_isolation", "Isolation");

    // رصيد لوحدة B فقط
    let dr_b = create_test_daily_report(&db, &unit_b, "2024-01-10");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_b),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 20.0,
            unit_price: 100.0,
            total_cost: 2000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        20.0,
        0.0,
        20.0,
        Some("Order"),
        Some(&dr_b),
        "2024-01-10T10:00:00Z",
        Some(&unit_b),
    );

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        20.0,
        20.0,
        0.0,
        Some("Consumption"),
        Some(&dr_b),
        "2024-01-10T12:00:00Z",
        Some(&unit_b),
    );

    // وحدة A لم يكن لها تاريخ سابق
    let dr_a = create_test_daily_report(&db, &unit_a, "2024-03-10");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_a),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 10.0,
            unit_price: 100.0,
            total_cost: 1000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        10.0,
        0.0,
        0.0,
        Some("Consumption"),
        Some(&dr_a),
        "2024-03-10T12:00:00Z",
        Some(&unit_a),
    );

    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_a, 2024, 3, false)
        .unwrap();

    let view = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_a, 2024, 3)
        .unwrap()
        .unwrap();

    let item = view
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    assert_eq!(item.opening_stock, 0.0, "يجب ألا يتسرب رصيد وحدة أخرى");
}

#[test]
fn test_in_for_correct_unit_is_counted() {
    let (db, unit_a) = setup_test_db_with_unit();
    let _unit_b = create_second_test_unit(&db);

    let product_id = create_test_product(&db, "prod_in_scope", "IN Scope Test");

    // تقرير لوحدة A
    let dr_a = create_test_daily_report(&db, &unit_a, "2024-01-10");
    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_a),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 10.0,
            unit_price: 100.0,
            total_cost: 1000.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // IN لوحدة A فقط
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_a),
        "2024-01-10T10:00:00Z",
        Some(&unit_a),
    );

    // خروج لوحدة A
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        10.0,
        100.0,
        90.0,
        Some("Consumption"),
        Some(&dr_a),
        "2024-01-15T12:00:00Z",
        Some(&unit_a),
    );

    // حساب snapshot لوحدة A
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_a, 2024, 1, false)
        .unwrap();
    let view_a = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_a, 2024, 1)
        .unwrap()
        .unwrap();
    let item_a = view_a
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // IN لوحدة A يجب أن يُحسب
    assert_eq!(item_a.total_in, 100.0, "Total IN for unit A should be 100");
    assert_eq!(item_a.total_out, 10.0, "Total OUT for unit A should be 10");
}

#[test]
fn test_in_not_double_counted_across_units() {
    let (db, unit_a) = setup_test_db_with_unit();
    let unit_b = create_second_test_unit(&db);

    let product_id = create_test_product(&db, "prod_no_double", "No Double Count");

    // تقارير لكل وحدة
    let dr_a = create_test_daily_report(&db, &unit_a, "2024-01-10");
    let dr_b = create_test_daily_report(&db, &unit_b, "2024-01-10");

    let report_repo = ReportRepository::new(db.executor());
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_a),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 5.0,
            unit_price: 100.0,
            total_cost: 500.0,
            fifo_layer_id: None,
        })
        .unwrap();
    report_repo
        .insert_raw_meal_item(&grpc_lib::models::DailyReportMealItem {
            id: Uuid::new_v4().to_string(),
            meal_id: test_breakfast_meal_id(&dr_b),
            product_id: product_id.clone(),
            product_name: "Test Product".to_string(),
            quantity: 5.0,
            unit_price: 100.0,
            total_cost: 500.0,
            fifo_layer_id: None,
        })
        .unwrap();

    // IN لوحدة A فقط (100)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        100.0,
        0.0,
        100.0,
        Some("Order"),
        Some(&dr_a),
        "2024-01-10T10:00:00Z",
        Some(&unit_a),
    );

    // IN لوحدة B فقط (50)
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::In,
        50.0,
        0.0,
        50.0,
        Some("Order"),
        Some(&dr_b),
        "2024-01-10T10:00:00Z",
        Some(&unit_b),
    );

    // خروج لكل وحدة
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        5.0,
        100.0,
        95.0,
        Some("Consumption"),
        Some(&dr_a),
        "2024-01-15T12:00:00Z",
        Some(&unit_a),
    );
    create_stock_movement(
        &db,
        &product_id,
        StockMovementType::Out,
        5.0,
        50.0,
        45.0,
        Some("Consumption"),
        Some(&dr_b),
        "2024-01-15T12:00:00Z",
        Some(&unit_b),
    );

    // حساب snapshot لكل وحدة
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_a, 2024, 1, false)
        .unwrap();
    grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .compute_and_store_unit_snapshot(&unit_b, 2024, 1, false)
        .unwrap();

    let view_a = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_a, 2024, 1)
        .unwrap()
        .unwrap();
    let item_a = view_a
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    let view_b = grpc_lib::application::services::InventorySnapshotService::new(db.executor())
        .get_unit_inventory_view(&unit_b, 2024, 1)
        .unwrap()
        .unwrap();
    let item_b = view_b
        .items
        .iter()
        .find(|i| i.product_id == product_id)
        .unwrap();

    // وحدة A: IN = 100 (فقط الخاص بها)
    assert_eq!(
        item_a.total_in, 100.0,
        "Unit A should only see its own IN (100)"
    );
    // وحدة B: IN = 50 (فقط الخاص بها)
    assert_eq!(
        item_b.total_in, 50.0,
        "Unit B should only see its own IN (50)"
    );
}

#[test]
fn test_confirm_order_sets_unit_id() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_confirm", "Confirm Order Test");

    // Create a user for the test (to satisfy FK constraint)
    let user_id = Uuid::new_v4().to_string();
    grpc_lib::repositories::UserRepository::new(db.executor())
        .insert_raw_user(
            &user_id,
            "test_admin",
            "Password123",
            "Admin",
            "WILAYA",
            &Utc::now().to_rfc3339(),
        )
        .expect("Failed to create user");

    // Set up entitlement: supplier + contract + product line + allocation with
    // agreed price (ADR-0055 / SEC-087-F). Orders require contract coverage.
    // The product must carry a SEC-087 unit/TVA configuration (fail closed) so
    // the authoritative pricing snapshot can be frozen at the agreement
    // boundary.
    db.get_connection()
        .execute(
            "UPDATE products SET purchase_unit = 1, consumption_unit = 1, conversion_factor = 1, tva_classification = 0 WHERE id = ?1",
            rusqlite::params![product_id],
        )
        .expect("configure product units");
    let now = Utc::now().to_rfc3339();
    let suppliers = grpc_lib::repositories::SupplierRepository::new(db.executor());
    let contracts = grpc_lib::repositories::ContractRepository::new(db.executor());
    let supplier_id = Uuid::new_v4().to_string();
    suppliers
        .insert_supplier(
            &supplier_id,
            &grpc_lib::models::CreateSupplierRequest {
                name: "مورد للتأكيد".to_string(),
                contact_info: None,
            },
            &now,
        )
        .unwrap();
    let contract_id = Uuid::new_v4().to_string();
    contracts
        .insert_contract(
            &contract_id,
            &grpc_lib::models::CreateContractRequest {
                unit_id: unit_id.clone(),
                supplier_id: supplier_id.clone(),
                fiscal_year: 2024,
                contract_reference: "CTR-CONF-2024".to_string(),
                notes: None,
            },
            &now,
        )
        .unwrap();
    let contract_product_id = Uuid::new_v4().to_string();
    contracts
        .insert_contract_product(
            &contract_product_id,
            &grpc_lib::models::AddContractProductRequest {
                contract_id: contract_id.clone(),
                product_id: product_id.clone(),
                proposed_price_ht: 40.0,
                agreed_price_ht: Some(40.0),
                contracted_quantity: 1000.0,
            },
            &now,
        )
        .unwrap();
    grpc_lib::application::services::ContractService::new(db.executor())
        .set_agreed_price_ht(&grpc_lib::models::SetAgreedPriceHtRequest {
            contract_product_id: contract_product_id.clone(),
            agreed_price_ht: 40.0,
        })
        .expect("freeze agreed HT price snapshot");
    let allocation_id = Uuid::new_v4().to_string();
    contracts
        .insert_allocation(
            &allocation_id,
            &contract_id,
            &contract_product_id,
            &unit_id,
            &product_id,
            2024,
            1000.0,
            &now,
        )
        .unwrap();

    // Create order (unit context + fiscal year are backend-provided)
    let order_req = CreateOrderRequest {
        reference_number: Some("CONF-001".to_string()),
        items: vec![OrderItemInput {
            product_id: product_id.clone(),
            quantity: 50.0,
        }],
    };
    let (order_id, _) = grpc_lib::application::services::OrderService::new(db.executor())
        .create_supplier_order(&order_req, &unit_id, 2024)
        .expect("Failed to create order");

    // Confirm order (unit context derives from the order itself)
    grpc_lib::application::services::OrderService::new(db.executor())
        .confirm_order_atomic(&order_id, "system", "test_admin")
        .expect("Failed to confirm order");

    // Verify the IN movement has unit_id set
    let movements = grpc_lib::application::services::StockMovementService::new(db.executor())
        .get_stock_movements(
            &grpc_lib::models::StockMovementFilters {
                product_id: Some(product_id.clone()),
                unit_id: Some(unit_id.clone()),
                reference_type: Some("Order".to_string()),
                reference_id: Some(order_id.clone().to_string()),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();

    assert!(
        !movements.movements.is_empty(),
        "IN movement should have unit_id set"
    );
}

#[test]
fn test_unit_cost_tracking_in_stock_movements() {
    let (db, unit_id) = setup_test_db_with_unit();
    let product_id = create_test_product(&db, "prod_cost_test", "Acquisition Cost Product");
    let executor = db.executor();
    let stock_service = grpc_lib::application::services::StockMovementService::new(executor);

    // 1. IN movement stores unit_cost
    let in_mov = NewStockMovement {
        product_id: product_id.clone(),
        movement_type: StockMovementType::In,
        quantity: 100.0,
        reference_type: Some("Order".to_string()),
        reference_id: Some("ORDER-001".to_string()),
        notes: Some("Acquisition test".to_string()),
        user_id: "system".to_string(),
        username: "test_user".to_string(),
        unit_id: Some(unit_id.clone()),
        unit_cost: Some(150.5),
    };
    let _in_id = stock_service.record_stock_movement(&in_mov).unwrap();

    // Verify stored IN movement unit_cost
    let movements_in = stock_service
        .get_stock_movements(
            &StockMovementFilters {
                product_id: Some(product_id.clone()),
                movement_type: Some("IN".to_string()),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(movements_in.movements[0].unit_cost, Some(150.5));

    // 2. OPENING movement stores unit_cost
    let open_mov = NewStockMovement {
        product_id: product_id.clone(),
        movement_type: StockMovementType::Opening,
        quantity: 50.0,
        reference_type: Some("Opening".to_string()),
        reference_id: Some("OPEN-001".to_string()),
        notes: Some("Opening balance test".to_string()),
        user_id: "system".to_string(),
        username: "test_user".to_string(),
        unit_id: Some(unit_id.clone()),
        unit_cost: Some(120.0),
    };
    let _open_id = stock_service.record_stock_movement(&open_mov).unwrap();

    // Verify stored OPENING movement unit_cost
    let movements_open = stock_service
        .get_stock_movements(
            &StockMovementFilters {
                product_id: Some(product_id.clone()),
                movement_type: Some("OPENING".to_string()),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(movements_open.movements[0].unit_cost, Some(120.0));

    // 3. OUT movement handles NULL safely
    let out_mov = NewStockMovement {
        product_id: product_id.clone(),
        movement_type: StockMovementType::Out,
        quantity: 10.0,
        reference_type: Some("Consumption".to_string()),
        reference_id: Some("CONS-001".to_string()),
        notes: Some("Consumption test".to_string()),
        user_id: "system".to_string(),
        username: "test_user".to_string(),
        unit_id: Some(unit_id.clone()),
        unit_cost: None,
    };
    let _out_id = stock_service.record_stock_movement(&out_mov).unwrap();

    // Verify stored OUT movement unit_cost
    let movements_out = stock_service
        .get_stock_movements(
            &StockMovementFilters {
                product_id: Some(product_id.clone()),
                movement_type: Some("OUT".to_string()),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(movements_out.movements[0].unit_cost, None);

    // 4. Serialization roundtrip
    let original_mov = movements_in.movements[0].clone();
    let serialized = serde_json::to_string(&original_mov).unwrap();
    let deserialized: StockMovement = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.unit_cost, Some(150.5));

    // 5. Sync export/import preserves unit_cost
    let movements_to_sync = stock_service
        .get_stock_movements_in_range("2020-01-01T00:00:00Z", "2100-01-01T00:00:00Z")
        .unwrap();

    // Create import sync service and run import on a clean/separate unit db
    let (sync_db, sync_unit_id) = setup_test_db_with_unit();
    create_test_product(&sync_db, "prod_cost_test", "Acquisition Cost Product");
    let sync_executor = sync_db.executor();
    let sync_svc = grpc_lib::application::services::SyncImportExecutionService::new(sync_executor);

    // Import the movements
    sync_svc
        .import_stock_movements(movements_to_sync, Some(&sync_unit_id))
        .unwrap();

    // Verify after import
    let imported_movements =
        grpc_lib::application::services::StockMovementService::new(sync_executor)
            .get_stock_movements(
                &StockMovementFilters {
                    product_id: Some("prod_cost_test".to_string()),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();

    let imported_in = imported_movements
        .movements
        .iter()
        .find(|m| m.movement_type == StockMovementType::In)
        .unwrap();
    assert_eq!(imported_in.unit_cost, Some(150.5));

    let imported_open = imported_movements
        .movements
        .iter()
        .find(|m| m.movement_type == StockMovementType::Opening)
        .unwrap();
    assert_eq!(imported_open.unit_cost, Some(120.0));

    let imported_out = imported_movements
        .movements
        .iter()
        .find(|m| m.movement_type == StockMovementType::Out)
        .unwrap();
    assert_eq!(imported_out.unit_cost, None);

    // 6. Legacy NULL values remain valid
    // Insert a movement with NULL unit_cost using raw SQL directly, to simulate legacy database rows
    executor.execute(
        "INSERT INTO stock_movements 
         (id, product_id, movement_type, quantity, balance_before, balance_after, timestamp, user_id, username, unit_id, fiscal_year, unit_cost)
         VALUES (?1, ?2, 'IN', 10.0, 0.0, 10.0, '2024-01-01T12:00:00Z', 'system', 'legacy', ?3, 2024, NULL)",
        rusqlite::params!["legacy-id-1", product_id, Some(&unit_id)],
    ).unwrap();

    let legacy_movement = stock_service
        .get_stock_movements(
            &StockMovementFilters {
                product_id: Some(product_id.clone()),
                movement_type: Some("IN".to_string()),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap()
        .movements
        .into_iter()
        .find(|m| m.id == "legacy-id-1")
        .unwrap();
    assert_eq!(legacy_movement.unit_cost, None);
}
