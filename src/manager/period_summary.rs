//! Plan-vs-actual for one budget period, with lazy rollover (plan R3 §6.3).
//!
//! Pure: callers fetch plans (category service), daily totals (entry service) and the rollover
//! policy (budget service), this module only does the arithmetic so it can be tested directly.
//!
//! carry_in(c, P) = 0                                       for RESET, or the first period of c
//!                = max(0, plan(c,P-1) + carry_in(c,P-1) - spent(c,P-1))   for CARRY_FORWARD
//! available(c, P) = plan(c, P) + carry_in(c, P)
//! Only the leftover moves forward; overspending never reduces the next period.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;

use crate::pb::service::category::{BudgetPlan, CategoryType};
use crate::pb::service::entry::PeriodTotalsRow;
use crate::pb::service::insight::{
    CategoryProgress, DailyPoint, PeriodSummary, ProgressStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rollover {
    Reset,
    CarryForward,
}

impl Rollover {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rollover::Reset => "reset",
            Rollover::CarryForward => "carry_forward",
        }
    }
}

pub const WARN_PCT: f64 = 80.0;

pub fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn period_index(periods: &[(NaiveDate, NaiveDate)], day: NaiveDate) -> Option<usize> {
    periods.iter().position(|(f, t)| day >= *f && day < *t)
}

fn status_for(spent: i64, available: i64) -> ProgressStatus {
    if spent > available {
        ProgressStatus::Over
    } else if available > 0 && (spent as f64 / available as f64) * 100.0 >= WARN_PCT {
        ProgressStatus::Warn
    } else {
        ProgressStatus::Ok
    }
}

fn pct(spent: i64, available: i64) -> f64 {
    if available > 0 {
        spent as f64 / available as f64 * 100.0
    } else {
        0.0
    }
}

/// `periods` and `plans` are oldest first and the same length; the last one is the target.
/// `rows` are entry totals grouped by (category, day) covering all periods.
pub fn compute(
    budget_id: &str,
    periods: &[(NaiveDate, NaiveDate)],
    plans: &[BudgetPlan],
    rows: &[PeriodTotalsRow],
    policy: Rollover,
    today: NaiveDate,
) -> PeriodSummary {
    assert!(!periods.is_empty() && periods.len() == plans.len());
    let t = periods.len() - 1;
    let (from, to) = periods[t];
    let target_plan = &plans[t];

    // Expense categories known to the target plan (the latest view of names/icons).
    let expense_cats: HashMap<&str, &crate::pb::service::category::CategoryPlan> = target_plan
        .categories
        .iter()
        .filter(|c| c.kind == CategoryType::Expense as i32)
        .map(|c| (c.category_id.as_str(), c))
        .collect();

    // spent[period][key]; unknown or empty category ids fall into "" (unallocated)
    let mut spent: Vec<HashMap<String, i64>> = vec![HashMap::new(); periods.len()];
    let mut daily: BTreeMap<NaiveDate, (i64, i64)> = BTreeMap::new();
    let (mut income, mut expense, mut tin, mut tout) = (0i64, 0i64, 0i64, 0i64);
    let mut first_active: Option<usize> = None;
    for r in rows {
        let Some(day) = parse_day(&r.day) else { continue };
        let Some(i) = period_index(periods, day) else { continue };
        if r.expense != 0 || r.income != 0 || r.transfer_in != 0 || r.transfer_out != 0 {
            first_active = Some(first_active.map_or(i, |f| f.min(i)));
        }
        let key = if expense_cats.contains_key(r.category_id.as_str()) {
            r.category_id.clone()
        } else {
            String::new()
        };
        *spent[i].entry(key).or_insert(0) += r.expense;
        if i == t {
            income += r.income;
            expense += r.expense;
            tin += r.transfer_in;
            tout += r.transfer_out;
            let d = daily.entry(day).or_insert((0, 0));
            d.0 += r.income;
            d.1 += r.expense;
        }
    }
    let first_active = first_active.unwrap_or(t);

    // planned[period][key] and the first period each key exists in
    let planned_of = |i: usize, key: &str| -> i64 {
        if key.is_empty() {
            return plans[i].unallocated_planned;
        }
        plans[i]
            .categories
            .iter()
            .find(|c| c.category_id == key)
            .map(|c| c.planned)
            .unwrap_or(0)
    };
    let start_of = |key: &str| -> usize {
        let created = expense_cats.get(key).map(|c| c.created_at).unwrap_or(0);
        let created_idx = if created > 0 {
            let day = chrono::DateTime::from_timestamp(created, 0)
                .map(|d| d.date_naive())
                .unwrap_or(from);
            periods
                .iter()
                .position(|(f, tt)| day >= *f && day < *tt)
                .unwrap_or(if day < periods[0].0 { 0 } else { periods.len() })
        } else {
            0
        };
        first_active.max(created_idx)
    };
    let carry_in = |key: &str| -> i64 {
        if policy == Rollover::Reset {
            return 0;
        }
        let begin = start_of(key);
        let mut carry = 0i64;
        for i in begin..t {
            let s = spent[i].get(key).copied().unwrap_or(0);
            carry = (planned_of(i, key) + carry - s).max(0);
        }
        carry
    };

    let mut keys: Vec<String> = expense_cats.keys().map(|k| k.to_string()).collect();
    keys.sort_by(|a, b| {
        let (ca, cb) = (expense_cats[a.as_str()], expense_cats[b.as_str()]);
        ca.name.to_lowercase().cmp(&cb.name.to_lowercase())
    });
    keys.push(String::new());

    let mut categories = Vec::new();
    let (mut total_available, mut total_spent) = (0i64, 0i64);
    let mut unallocated_spent = 0i64;
    let mut seen = HashSet::new();
    for key in keys {
        if !seen.insert(key.clone()) {
            continue;
        }
        let planned = planned_of(t, &key);
        let carry = carry_in(&key);
        let s = spent[t].get(&key).copied().unwrap_or(0);
        let archived = expense_cats.get(key.as_str()).map(|c| c.archived).unwrap_or(false);
        if archived && planned == 0 && carry == 0 && s == 0 {
            continue;
        }
        let is_unallocated = key.is_empty();
        if is_unallocated {
            unallocated_spent = s;
            if planned == 0 && carry == 0 && s == 0 {
                continue;
            }
        }
        let available = planned + carry;
        total_available += available;
        total_spent += s;
        let meta = expense_cats.get(key.as_str());
        categories.push(CategoryProgress {
            category_id: key.clone(),
            name: meta.map(|c| c.name.clone()).unwrap_or_default(),
            icon: meta.map(|c| c.icon.clone()).unwrap_or_default(),
            color: meta.map(|c| c.color.clone()).unwrap_or_default(),
            planned,
            carry_in: carry,
            available,
            spent: s,
            pct: pct(s, available),
            status: status_for(s, available) as i32,
        });
    }

    let total_days = (to - from).num_days().max(1);
    let days_left = if today < from {
        total_days
    } else if today >= to {
        0
    } else {
        (to - today).num_days()
    };
    let remaining = total_available - total_spent;
    let safe_daily_spend = if days_left > 0 { remaining.max(0) / days_left } else { 0 };
    let projected_pct = if total_available <= 0 || today < from {
        0.0
    } else if today >= to {
        pct(total_spent, total_available)
    } else {
        let elapsed = ((today - from).num_days() + 1).max(1);
        let projected = total_spent as f64 / elapsed as f64 * total_days as f64;
        projected / total_available as f64 * 100.0
    };

    // Continuous daily series up to today (or the whole period once it is over)
    let last_day = if today < from {
        None
    } else if today >= to {
        Some(to - chrono::Duration::days(1))
    } else {
        Some(today)
    };
    let mut daily_points = Vec::new();
    if let Some(last) = last_day {
        let mut d = from;
        while d <= last {
            let (inc, exp) = daily.get(&d).copied().unwrap_or((0, 0));
            daily_points.push(DailyPoint {
                date: d.format("%Y-%m-%d").to_string(),
                income: inc,
                expense: exp,
            });
            d += chrono::Duration::days(1);
        }
    }

    PeriodSummary {
        budget_id: budget_id.to_string(),
        period_from: from.format("%Y-%m-%d").to_string(),
        period_to: to.format("%Y-%m-%d").to_string(),
        days_left: days_left as i32,
        income,
        expense,
        transfer_in: tin,
        transfer_out: tout,
        total_available,
        total_spent,
        remaining,
        safe_daily_spend,
        projected_pct,
        categories,
        unallocated_spent,
        daily: daily_points,
        rollover_policy: policy.as_str().to_string(),
    }
}
