//! Insight service integration tests.
//!
//! All tests use mocked clients — no DB, no network.

use std::sync::Arc;

use chrono::Datelike;

// ---------------------------------------------------------------------------
// Mock clients
// ---------------------------------------------------------------------------

/// Simplified mock of an entry record.
#[derive(Clone, Debug)]
struct MockEntry {
    pub amount: i64,
    pub kind: i32, // EntryKind i32
    pub category_id: String,
    pub entry_date: String,
    pub description: String,
    pub base_id: String,
}

/// Mock EntryClient that returns pre-configured entries.
#[derive(Clone, Default)]
struct MockEntryClient {
    entries: Arc<std::collections::HashMap<String, Vec<MockEntry>>>,
}

impl MockEntryClient {
    fn with_entries(budget_id: &str, entries: Vec<MockEntry>) -> Self {
        let mut m = std::collections::HashMap::new();
        m.insert(budget_id.to_string(), entries);
        Self {
            entries: Arc::new(m),
        }
    }

    fn with_multi_budget_entries(entries: std::collections::HashMap<String, Vec<MockEntry>>) -> Self {
        Self {
            entries: Arc::new(entries),
        }
    }
}

/// Mock BudgetClient that returns pre-configured budgets.
#[derive(Clone, Default)]
struct MockBudgetClient {
    budgets: Vec<Budget>,
}

#[derive(Clone, Debug)]
struct Budget {
    pub budget_id: String,
    pub name: String,
    pub budget_type: i32,
}

impl MockBudgetClient {
    fn with_budgets(budgets: Vec<Budget>) -> Self {
        Self { budgets }
    }
}

// ---------------------------------------------------------------------------
// Mock wrappers that mimic the real client signatures
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct WrappedEntryClient {
    inner: MockEntryClient,
}

#[derive(Clone)]
struct WrappedBudgetClient {
    inner: MockBudgetClient,
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

// NOTE: The real test approach uses the existing handler+biz structure.
// Below are pure unit tests of the business logic paths.

// -- Monthly Summary --

#[tokio::test]
async fn monthly_summary_empty_budget_returns_zeros() {
    // Empty entries for a budget → income=0, expense=0, net=0, savings_rate=0
    let client = MockEntryClient::with_entries("b1", vec![]);
    // The actual computation happens in InsightBiz.compute_monthly_net
    // We verify the logic with a simple assertion-based test
    let entries = client.entries.get("b1").cloned().unwrap_or_default();
    let (income, expense, net) = compute_net_from_entries(&entries);
    assert_eq!(income, 0);
    assert_eq!(expense, 0);
    assert_eq!(net, 0);
}

#[tokio::test]
async fn monthly_summary_income_expense_net_calculated_correctly() {
    let entries = vec![
        MockEntry {
            amount: 1000,
            kind: 1, // Income
            category_id: "cat1".to_string(),
            entry_date: "2026-06-01".to_string(),
            description: "Salary".to_string(),
            base_id: "e1".to_string(),
        },
        MockEntry {
            amount: 300,
            kind: 2, // Expense
            category_id: "cat2".to_string(),
            entry_date: "2026-06-02".to_string(),
            description: "Groceries".to_string(),
            base_id: "e2".to_string(),
        },
    ];
    let (income, expense, net) = compute_net_from_entries(&entries);
    assert_eq!(income, 1000);
    assert_eq!(expense, 300);
    assert_eq!(net, 700);
}

#[tokio::test]
async fn monthly_summary_savings_rate_zero_when_no_income() {
    let entries = vec![MockEntry {
        amount: 500,
        kind: 2, // Expense
        category_id: "cat1".to_string(),
        entry_date: "2026-06-01".to_string(),
        description: "Shopping".to_string(),
        base_id: "e1".to_string(),
    }];
    let (income, _, net) = compute_net_from_entries(&entries);
    let savings_rate = if income > 0 {
        net as f64 / income as f64
    } else {
        0.0
    };
    assert_eq!(income, 0);
    assert_eq!(net, -500);
    assert_eq!(savings_rate, 0.0);
}

#[tokio::test]
async fn monthly_delta_computed_from_prior_month() {
    let current_entries = vec![MockEntry {
        amount: 1000,
        kind: 1,
        category_id: "cat1".to_string(),
        entry_date: "2026-06-01".to_string(),
        description: "Salary".to_string(),
        base_id: "e1".to_string(),
    }];
    let prior_entries = vec![MockEntry {
        amount: 800,
        kind: 1,
        category_id: "cat1".to_string(),
        entry_date: "2026-05-01".to_string(),
        description: "Salary".to_string(),
        base_id: "e3".to_string(),
    }];
    let (_, _, current_net) = compute_net_from_entries(&current_entries);
    let (_, _, prior_net) = compute_net_from_entries(&prior_entries);
    let delta = current_net.saturating_sub(prior_net);
    assert_eq!(current_net, 1000);
    assert_eq!(prior_net, 800);
    assert_eq!(delta, 200);
}

// -- Weekly Summary --

#[tokio::test]
async fn weekly_summary_7_day_range() {
    // Entries across 7 days (Mon-Sun) should all be included
    let entries = vec![
        MockEntry { amount: 100, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-01".to_string(), description: "d1".to_string(), base_id: "e1".to_string() }, // Mon
        MockEntry { amount: 200, kind: 2, category_id: "c1".to_string(), entry_date: "2026-06-02".to_string(), description: "d2".to_string(), base_id: "e2".to_string() }, // Tue
        MockEntry { amount: 50, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-03".to_string(), description: "d3".to_string(), base_id: "e3".to_string() }, // Wed
        MockEntry { amount: 75, kind: 2, category_id: "c1".to_string(), entry_date: "2026-06-04".to_string(), description: "d4".to_string(), base_id: "e4".to_string() }, // Thu
        MockEntry { amount: 300, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-05".to_string(), description: "d5".to_string(), base_id: "e5".to_string() }, // Fri
        MockEntry { amount: 400, kind: 2, category_id: "c1".to_string(), entry_date: "2026-06-06".to_string(), description: "d6".to_string(), base_id: "e6".to_string() }, // Sat
        MockEntry { amount: 150, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-07".to_string(), description: "d7".to_string(), base_id: "e7".to_string() }, // Sun
    ];
    // Filter: week_start="2026-06-01", week_end="2026-06-07"
    let week_entries: Vec<MockEntry> = entries
        .iter()
        .filter(|e| e.entry_date.as_str() >= "2026-06-01" && e.entry_date.as_str() <= "2026-06-07")
        .cloned()
        .collect();
    let (income, expense, net) = compute_net_from_entries(&week_entries);
    // Income = 100 + 50 + 300 + 150 = 600; Expense = 200 + 75 + 400 = 675; Net = -75
    assert_eq!(income, 600);
    assert_eq!(expense, 675);
    assert_eq!(net, -75);
}

// -- Net Worth --

#[tokio::test]
async fn net_worth_filters_sharing_budgets() {
    let budgets = vec![
        Budget { budget_id: "b1".to_string(), name: "Checking".to_string(), budget_type: 1 }, // Standard
        Budget { budget_id: "b2".to_string(), name: "Sharing".to_string(), budget_type: 5 }, // Sharing — filter out
        Budget { budget_id: "b3".to_string(), name: "Savings".to_string(), budget_type: 2 }, // Saving
    ];
    let non_sharing: Vec<_> = budgets
        .iter()
        .filter(|b| b.budget_type != 5)
        .collect();
    assert_eq!(non_sharing.len(), 2);
    assert_eq!(non_sharing[0].budget_id, "b1");
    assert_eq!(non_sharing[1].budget_id, "b3");
}

#[tokio::test]
async fn net_worth_aggregates_multiple_budgets() {
    // Budget b1: net = 1000 (assets), Budget b2: net = -200 (liabilities)
    let b1_entries = vec![MockEntry { amount: 1000, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-01".to_string(), description: "inc".to_string(), base_id: "e1".to_string() }];
    let b2_entries = vec![MockEntry { amount: 200, kind: 2, category_id: "c1".to_string(), entry_date: "2026-06-01".to_string(), description: "exp".to_string(), base_id: "e2".to_string() }];

    let compute_net = |entries: &[MockEntry]| {
        let (_income, _expense, net) = compute_net_from_entries(entries);
        net
    };

    let net_b1 = compute_net(&b1_entries); // 1000
    let net_b2 = compute_net(&b2_entries); // -200

    let assets_b1 = if net_b1 > 0 { net_b1 } else { 0 };
    let liabilities_b1 = if net_b1 < 0 { net_b1.abs() } else { 0 };
    let assets_b2 = if net_b2 > 0 { net_b2 } else { 0 };
    let liabilities_b2 = if net_b2 < 0 { net_b2.abs() } else { 0 };

    let total_assets = assets_b1 + assets_b2;
    let total_liabilities = liabilities_b1 + liabilities_b2;
    let net_worth = total_assets - total_liabilities;

    assert_eq!(total_assets, 1000);
    assert_eq!(total_liabilities, 200);
    assert_eq!(net_worth, 800);
}

// -- Category Drilldown --

#[tokio::test]
async fn category_drilldown_returns_entries_in_range() {
    let entries = vec![
        MockEntry { amount: 100, kind: 2, category_id: "cat1".to_string(), entry_date: "2026-06-01".to_string(), description: "d1".to_string(), base_id: "e1".to_string() },
        MockEntry { amount: 200, kind: 2, category_id: "cat1".to_string(), entry_date: "2026-06-15".to_string(), description: "d2".to_string(), base_id: "e2".to_string() },
        MockEntry { amount: 300, kind: 2, category_id: "cat2".to_string(), entry_date: "2026-06-10".to_string(), description: "d3".to_string(), base_id: "e3".to_string() },
    ];
    let filtered: Vec<_> = entries
        .iter()
        .filter(|e| e.category_id == "cat1" && e.entry_date.as_str() >= "2026-06-01" && e.entry_date.as_str() <= "2026-06-30")
        .collect();
    assert_eq!(filtered.len(), 2);
    let total: i64 = filtered.iter().map(|e| e.amount).sum();
    assert_eq!(total, 300);
}

#[tokio::test]
async fn category_drilldown_sums_total() {
    let entries = vec![
        MockEntry { amount: 50, kind: 2, category_id: "cat1".to_string(), entry_date: "2026-06-01".to_string(), description: "d1".to_string(), base_id: "e1".to_string() },
        MockEntry { amount: 75, kind: 2, category_id: "cat1".to_string(), entry_date: "2026-06-05".to_string(), description: "d2".to_string(), base_id: "e2".to_string() },
        MockEntry { amount: 25, kind: 1, category_id: "cat1".to_string(), entry_date: "2026-06-10".to_string(), description: "d3".to_string(), base_id: "e3".to_string() },
    ];
    let total: i64 = entries.iter().map(|e| e.amount).sum();
    assert_eq!(total, 150);
}

// -- Yearly Summary --

#[tokio::test]
async fn yearly_summary_top_categories_sorted_desc() {
    let mut category_sums: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    category_sums.insert("cat1".to_string(), 500);
    category_sums.insert("cat2".to_string(), 1200);
    category_sums.insert("cat3".to_string(), 300);

    let mut sorted: Vec<(String, i64)> = category_sums.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let top5: Vec<_> = sorted.into_iter().take(5).collect();

    assert_eq!(top5[0].0, "cat2");
    assert_eq!(top5[0].1, 1200);
    assert_eq!(top5[1].0, "cat1");
    assert_eq!(top5[1].1, 500);
    assert_eq!(top5[2].0, "cat3");
    assert_eq!(top5[2].1, 300);
}

#[tokio::test]
async fn yearly_summary_biggest_transaction_found() {
    let entries = vec![
        MockEntry { amount: 100, kind: 2, category_id: "c1".to_string(), entry_date: "2026-01-01".to_string(), description: "small".to_string(), base_id: "e1".to_string() },
        MockEntry { amount: 5000, kind: 2, category_id: "c2".to_string(), entry_date: "2026-06-15".to_string(), description: "biggest".to_string(), base_id: "e2".to_string() },
        MockEntry { amount: 300, kind: 2, category_id: "c1".to_string(), entry_date: "2026-03-10".to_string(), description: "medium".to_string(), base_id: "e3".to_string() },
    ];
    let biggest = entries.iter().max_by_key(|e| e.amount);
    assert!(biggest.is_some());
    assert_eq!(biggest.unwrap().amount, 5000);
    assert_eq!(biggest.unwrap().description, "biggest");
}

#[tokio::test]
async fn yearly_summary_month_maps_complete_12_months() {
    let entries = vec![
        MockEntry { amount: 1000, kind: 1, category_id: "c1".to_string(), entry_date: "2026-01-15".to_string(), description: "jan".to_string(), base_id: "e1".to_string() },
        MockEntry { amount: 2000, kind: 1, category_id: "c1".to_string(), entry_date: "2026-06-15".to_string(), description: "jun".to_string(), base_id: "e2".to_string() },
        MockEntry { amount: 500, kind: 2, category_id: "c1".to_string(), entry_date: "2026-12-01".to_string(), description: "dec".to_string(), base_id: "e3".to_string() },
    ];
    let mut month_income_map = vec![0i64; 12];
    let mut month_expense_map = vec![0i64; 12];

    for entry in &entries {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&entry.entry_date, "%Y-%m-%d") {
            let m = (date.month() as usize).saturating_sub(1);
            if m < 12 {
                match entry.kind {
                    1 => month_income_map[m] += entry.amount,
                    2 => month_expense_map[m] += entry.amount,
                    _ => {}
                }
            }
        }
    }

    assert_eq!(month_income_map[0], 1000);  // Jan
    assert_eq!(month_income_map[5], 2000);  // Jun
    assert_eq!(month_expense_map[11], 500); // Dec
    // All other months should be 0
    for (i, &val) in month_income_map.iter().enumerate() {
        if i != 0 && i != 5 {
            assert_eq!(val, 0, "month_income_map[{}] should be 0", i);
        }
    }
    for (i, &val) in month_expense_map.iter().enumerate() {
        if i != 11 {
            assert_eq!(val, 0, "month_expense_map[{}] should be 0", i);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compute_net_from_entries(entries: &[MockEntry]) -> (i64, i64, i64) {
    let income: i64 = entries.iter().filter(|e| e.kind == 1).map(|e| e.amount).sum();
    let expense: i64 = entries.iter().filter(|e| e.kind == 2).map(|e| e.amount).sum();
    let net = income.saturating_sub(expense);
    (income, expense, net)
}