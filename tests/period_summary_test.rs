use chrono::NaiveDate;
use insight::manager::period_summary::{compute, Rollover};
use insight::pb::service::category::{BudgetPlan, CategoryPlan, CategoryType};
use insight::pb::service::entry::PeriodTotalsRow;
use insight::pb::service::insight::ProgressStatus;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn month(y: i32, m: u32) -> (NaiveDate, NaiveDate) {
    let next = if m == 12 {
        d(y + 1, 1, 1)
    } else {
        d(y, m + 1, 1)
    };
    (d(y, m, 1), next)
}

fn cat(id: &str, planned: i64) -> CategoryPlan {
    CategoryPlan {
        category_id: id.into(),
        name: id.into(),
        kind: CategoryType::Expense as i32,
        planned,
        default_planned: planned,
        ..Default::default()
    }
}

fn plan(cats: Vec<CategoryPlan>, unallocated: i64) -> BudgetPlan {
    BudgetPlan {
        budget_id: "b".into(),
        categories: cats,
        unallocated_planned: unallocated,
        ..Default::default()
    }
}

fn row(cat: &str, day: &str, expense: i64) -> PeriodTotalsRow {
    PeriodTotalsRow {
        budget_id: "b".into(),
        category_id: cat.into(),
        day: day.into(),
        expense,
        ..Default::default()
    }
}

fn find<'a>(
    s: &'a insight::pb::service::insight::PeriodSummary,
    id: &str,
) -> &'a insight::pb::service::insight::CategoryProgress {
    s.categories.iter().find(|c| c.category_id == id).unwrap()
}

#[test]
fn reset_policy_never_carries() {
    let periods = [month(2026, 8), month(2026, 9), month(2026, 10)];
    let plans: Vec<_> = (0..3).map(|_| plan(vec![cat("food", 1000)], 0)).collect();
    let rows = [
        row("food", "2026-08-10", 200),
        row("food", "2026-10-05", 300),
    ];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    let food = find(&s, "food");
    assert_eq!(food.carry_in, 0);
    assert_eq!(food.available, 1000);
    assert_eq!(food.spent, 300);
}

#[test]
fn carry_forward_moves_only_the_leftover() {
    let periods = [month(2026, 8), month(2026, 9), month(2026, 10)];
    let plans: Vec<_> = (0..3).map(|_| plan(vec![cat("food", 1000)], 0)).collect();
    // Aug leftover 400, Sep overspent by 500 -> overspend is NOT carried, leftover resets to 0
    let rows = [
        row("food", "2026-08-10", 600),
        row("food", "2026-09-10", 1500),
    ];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::CarryForward,
        d(2026, 10, 10),
    );
    assert_eq!(find(&s, "food").carry_in, 0);

    // Only August is under-spent: 1000-600 = 400 flows into September, then 1400-100 = 1300 into October
    let rows = [
        row("food", "2026-08-10", 600),
        row("food", "2026-09-10", 100),
    ];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::CarryForward,
        d(2026, 10, 10),
    );
    assert_eq!(find(&s, "food").carry_in, 1300);
    assert_eq!(find(&s, "food").available, 2300);
}

#[test]
fn carry_starts_when_the_budget_started_being_used() {
    let periods = [month(2026, 8), month(2026, 9), month(2026, 10)];
    let plans: Vec<_> = (0..3).map(|_| plan(vec![cat("food", 1000)], 0)).collect();
    // first activity in September: August (empty, before the budget was used) must not add 1000
    let rows = [row("food", "2026-09-10", 100)];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::CarryForward,
        d(2026, 10, 10),
    );
    assert_eq!(find(&s, "food").carry_in, 900);
}

#[test]
fn category_created_late_does_not_inherit_earlier_periods() {
    let periods = [month(2026, 8), month(2026, 9), month(2026, 10)];
    let mut late = cat("travel", 500);
    // created on 15 Oct 2026
    late.created_at = chrono::NaiveDate::from_ymd_opt(2026, 10, 15)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();
    let plans: Vec<_> = (0..3)
        .map(|_| plan(vec![cat("food", 1000), late.clone()], 0))
        .collect();
    let rows = [row("food", "2026-08-10", 1)];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::CarryForward,
        d(2026, 10, 20),
    );
    assert_eq!(find(&s, "travel").carry_in, 0);
    assert_eq!(find(&s, "travel").available, 500);
}

#[test]
fn uncategorised_and_unknown_category_spend_counts_as_unallocated() {
    let periods = [month(2026, 10)];
    let plans = [plan(vec![cat("food", 1000)], 300)];
    let rows = [
        row("", "2026-10-02", 50),
        row("deleted-cat", "2026-10-03", 70),
        row("food", "2026-10-04", 100),
    ];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert_eq!(s.unallocated_spent, 120);
    assert_eq!(s.total_spent, 220);
    assert_eq!(s.total_available, 1300);
    assert_eq!(s.remaining, 1080);
    let un = find(&s, "");
    assert_eq!((un.planned, un.spent), (300, 120));
}

#[test]
fn status_thresholds() {
    let periods = [month(2026, 10)];
    let plans = [plan(
        vec![cat("a", 100), cat("b", 100), cat("c", 100), cat("z", 0)],
        0,
    )];
    let rows = [
        row("a", "2026-10-02", 79),
        row("b", "2026-10-02", 80),
        row("c", "2026-10-02", 101),
        row("z", "2026-10-02", 1),
    ];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert_eq!(find(&s, "a").status, ProgressStatus::Ok as i32);
    assert_eq!(find(&s, "b").status, ProgressStatus::Warn as i32);
    assert_eq!(find(&s, "c").status, ProgressStatus::Over as i32);
    // nothing planned but money spent is over
    assert_eq!(find(&s, "z").status, ProgressStatus::Over as i32);
}

#[test]
fn safe_daily_spend_and_projection_for_the_running_period() {
    let periods = [month(2026, 10)];
    let plans = [plan(vec![cat("food", 3100)], 0)];
    // 10 days elapsed (1..=10), 1000 spent, 22 days left (11 Oct .. 1 Nov exclusive = 22)
    let rows = [row("food", "2026-10-03", 1000)];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert_eq!(s.days_left, 22);
    assert_eq!(s.remaining, 2100);
    assert_eq!(s.safe_daily_spend, 2100 / 22);
    // 1000 / 10 days * 31 days = 3100 -> 100%
    assert!((s.projected_pct - 100.0).abs() < 1e-9);
    assert_eq!(s.daily.len(), 10);
    assert_eq!(s.daily[2].expense, 1000);
}

#[test]
fn finished_period_has_no_days_left_and_uses_actual_pct() {
    let periods = [month(2026, 9)];
    let plans = [plan(vec![cat("food", 1000)], 0)];
    let rows = [row("food", "2026-09-03", 500)];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert_eq!(s.days_left, 0);
    assert_eq!(s.safe_daily_spend, 0);
    assert!((s.projected_pct - 50.0).abs() < 1e-9);
    assert_eq!(s.daily.len(), 30);
}

#[test]
fn transfers_are_reported_but_never_counted_as_spending() {
    let periods = [month(2026, 10)];
    let plans = [plan(vec![cat("food", 1000)], 0)];
    let mut r = row("", "2026-10-02", 0);
    r.transfer_out = 700;
    r.transfer_in = 200;
    let mut inc = row("", "2026-10-03", 0);
    inc.income = 5000;
    let s = compute(
        "b",
        &periods,
        &plans,
        &[r, inc],
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert_eq!((s.transfer_out, s.transfer_in), (700, 200));
    assert_eq!(s.income, 5000);
    assert_eq!(s.total_spent, 0);
    assert_eq!(s.expense, 0);
}

#[test]
fn archived_category_is_hidden_unless_it_has_money() {
    let periods = [month(2026, 10)];
    let mut old = cat("old", 0);
    old.archived = true;
    let mut old2 = cat("old2", 0);
    old2.archived = true;
    let plans = [plan(vec![old, old2], 0)];
    let rows = [row("old2", "2026-10-02", 10)];
    let s = compute(
        "b",
        &periods,
        &plans,
        &rows,
        Rollover::Reset,
        d(2026, 10, 10),
    );
    assert!(s.categories.iter().all(|c| c.category_id != "old"));
    assert_eq!(find(&s, "old2").spent, 10);
}
