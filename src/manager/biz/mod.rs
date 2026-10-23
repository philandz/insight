use crate::pb::service::insight::{
    BudgetNetWorth, CategoryDrilldownResponse, CategoryTotal, DrilledEntry,
    CategoryDrilldownRequest, MonthlySummaryRequest, MonthlySummaryResponse,
    NetWorthRequest, NetWorthResponse, WeeklySummaryRequest, WeeklySummaryResponse,
    YearlySummaryRequest, YearlySummaryResponse,
};
use crate::pb::service::entry::EntryKind;
use crate::manager::client::{BudgetClient, EntryClient};
use crate::manager::repository::InsightRepository;
use std::sync::Arc;
use tonic::Status;
use tokio::sync::Mutex;

/// Sums of one set of entries. Transfers are tracked separately and never touch
/// income/expense/net (plan I3).
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Totals {
    pub income: i64,
    pub expense: i64,
    pub transfer_in: i64,
    pub transfer_out: i64,
    pub saved_to_funds: i64,
}

impl Totals {
    pub fn net(&self) -> i64 {
        self.income.saturating_sub(self.expense)
    }
    pub fn cash_flow(&self) -> i64 {
        self.net()
            .saturating_add(self.transfer_in)
            .saturating_sub(self.transfer_out)
    }
}

pub fn tally(entries: &[crate::pb::service::entry::Entry]) -> Totals {
    let mut t = Totals::default();
    for e in entries {
        match EntryKind::try_from(e.kind).unwrap_or(EntryKind::Unspecified) {
            EntryKind::Income => t.income += e.amount,
            EntryKind::Expense => t.expense += e.amount,
            EntryKind::TransferIn => t.transfer_in += e.amount,
            EntryKind::TransferOut => {
                t.transfer_out += e.amount;
                if e.counterparty_kind.as_deref() == Some("fund_cash") {
                    t.saved_to_funds += e.amount;
                }
            }
            _ => {}
        }
    }
    t
}

pub struct InsightBiz {
    pub repo: Arc<InsightRepository>,
    pub entry_client: Arc<Mutex<EntryClient>>,
    pub budget_client: Arc<Mutex<BudgetClient>>,
}

impl InsightBiz {
    pub fn new(
        repo: Arc<InsightRepository>,
        entry_client: EntryClient,
        budget_client: BudgetClient,
    ) -> Self {
        Self {
            repo,
            entry_client: Arc::new(Mutex::new(entry_client)),
            budget_client: Arc::new(Mutex::new(budget_client)),
        }
    }

    fn map_err(e: impl ToString) -> Status {
        Status::internal(e.to_string())
    }

    async fn compute_monthly_totals(
        &self,
        user_id: &str,
        budget_id: &str,
        year: i32,
        month: u32,
    ) -> Result<Totals, Status> {
        let date_from = format!("{}-{:02}-01", year, month);
        let last_day = last_day_of_month(year, month);
        let date_to = format!("{}-{:02}-{}", year, month, last_day);

        let mut client = self.entry_client.lock().await;
        let resp = client
            .list_entries(user_id, budget_id, &date_from, &date_to, None, 9999)
            .await
            .map_err(Self::map_err)?;

        Ok(tally(&resp.entries))
    }

    pub async fn get_monthly_summary(
        &self,
        user_id: &str,
        req: MonthlySummaryRequest,
    ) -> Result<MonthlySummaryResponse, Status> {
        let cur = self
            .compute_monthly_totals(user_id, &req.budget_id, req.year, req.month as u32)
            .await?;
        let (income, expense, net) = (cur.income, cur.expense, cur.net());
        let savings_rate = if income > 0 { net as f64 / income as f64 } else { 0.0 };

        let (prior_year, prior_month) = if req.month == 1 {
            (req.year - 1, 12)
        } else {
            (req.year, req.month - 1)
        };
        let prior_net = self
            .compute_monthly_totals(user_id, &req.budget_id, prior_year, prior_month as u32)
            .await?
            .net();

        let delta = net.saturating_sub(prior_net);

        Ok(MonthlySummaryResponse {
            income,
            expense,
            net,
            savings_rate,
            delta_vs_last_month: delta,
            prior_net,
            transfer_in: cur.transfer_in,
            transfer_out: cur.transfer_out,
            cash_flow: cur.cash_flow(),
            saved_to_funds: cur.saved_to_funds,
        })
    }

    async fn compute_weekly_totals(&self, user_id: &str, budget_id: &str, week_start: &str) -> Result<Totals, Status> {
        let date_from = week_start.to_string();
        let base = chrono::NaiveDate::parse_from_str(week_start, "%Y-%m-%d")
            .map_err(|e| Self::map_err(e.to_string()))?;
        let date_to = (base + chrono::Duration::days(6)).format("%Y-%m-%d").to_string();

        let mut client = self.entry_client.lock().await;
        let resp = client
            .list_entries(user_id, budget_id, &date_from, &date_to, None, 9999)
            .await
            .map_err(Self::map_err)?;

        Ok(tally(&resp.entries))
    }

    pub async fn get_weekly_summary(
        &self,
        user_id: &str,
        req: WeeklySummaryRequest,
    ) -> Result<WeeklySummaryResponse, Status> {
        let t = self.compute_weekly_totals(user_id, &req.budget_id, &req.week_start).await?;
        Ok(WeeklySummaryResponse {
            income: t.income,
            expense: t.expense,
            net: t.net(),
            transfer_in: t.transfer_in,
            transfer_out: t.transfer_out,
            cash_flow: t.cash_flow(),
        })
    }

    pub async fn get_net_worth(
        &self,
        user_id: &str,
        req: NetWorthRequest,
    ) -> Result<NetWorthResponse, Status> {
        use crate::pb::service::budget::BudgetType;

        let mut client = self.budget_client.lock().await;
        let resp = client.list_budgets(&req.org_id, user_id).await.map_err(Self::map_err)?;

        let mut budgets = vec![];
        for b in resp.budgets.iter() {
            if BudgetType::try_from(b.budget_type).unwrap_or(BudgetType::Unspecified) == BudgetType::Sharing {
                continue;
            }

            let budget_id = b.base.as_ref()
                .map(|base| base.id.clone())
                .unwrap_or_default();

            let mut entry_client = self.entry_client.lock().await;
            let resp_entries = entry_client
                .list_entries(user_id, &budget_id, "2000-01-01", "2099-12-31", None, 9999)
                .await
                .map_err(Self::map_err)?;

            let mut income = 0i64;
            let mut expense = 0i64;
            for entry in resp_entries.entries.iter() {
                let kind = EntryKind::try_from(entry.kind).unwrap_or(EntryKind::Unspecified);
                match kind {
                    EntryKind::Income => income += entry.amount,
                    EntryKind::Expense => expense += entry.amount,
                    _ => {}
                }
            }
            let net = income.saturating_sub(expense);
            let assets = if net > 0 { net } else { 0 };
            let liabilities = if net < 0 { net.abs() } else { 0 };
            let name = b.name.clone();

            budgets.push(BudgetNetWorth {
                budget_id,
                name,
                assets,
                liabilities,
                net,
            });
        }

        let total_assets: i64 = budgets.iter().map(|b| b.assets).sum();
        let total_liabilities: i64 = budgets.iter().map(|b| b.liabilities).sum();
        let net_worth = total_assets.saturating_sub(total_liabilities);

        Ok(NetWorthResponse {
            total_assets,
            total_liabilities,
            net_worth,
            budgets,
        })
    }

    pub async fn get_category_drilldown(
        &self,
        user_id: &str,
        req: CategoryDrilldownRequest,
    ) -> Result<CategoryDrilldownResponse, Status> {
        let mut client = self.entry_client.lock().await;
        let resp = client
            .list_entries(user_id, &req.budget_id, &req.date_from, &req.date_to, Some(req.category_id.as_str()), 500)
            .await
            .map_err(Self::map_err)?;

        let mut total = 0i64;
        let entries: Vec<DrilledEntry> = resp
            .entries
            .iter()
            .map(|e| {
                total += e.amount;
                let kind = EntryKind::try_from(e.kind).unwrap_or(EntryKind::Unspecified);
                let kind_str = match kind {
                    EntryKind::Income => "INCOME",
                    EntryKind::Expense => "EXPENSE",
                    _ => "UNKNOWN",
                };
                let entry_id = e.base.as_ref()
                    .map(|b| b.id.clone())
                    .unwrap_or_default();
                DrilledEntry {
                    entry_id,
                    entry_date: e.entry_date.clone(),
                    amount: e.amount,
                    description: e.description.clone(),
                    kind: kind_str.to_string(),
                }
            })
            .collect();

        Ok(CategoryDrilldownResponse { entries, total })
    }

    pub async fn get_yearly_summary(
        &self,
        user_id: &str,
        req: YearlySummaryRequest,
    ) -> Result<YearlySummaryResponse, Status> {
        use chrono::Datelike;
        let date_from = format!("{}-01-01", req.year);
        let date_to = format!("{}-12-31", req.year);

        let mut client = self.entry_client.lock().await;
        let resp = client
            .list_entries(user_id, &req.budget_id, &date_from, &date_to, None, 9999)
            .await
            .map_err(Self::map_err)?;

        let mut category_sums: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut month_income_map = vec![0i64; 12];
        let mut month_expense_map = vec![0i64; 12];
        let mut biggest: Option<&crate::pb::service::entry::Entry> = None;

        for entry in resp.entries.iter() {
            if !entry.category_id.is_empty() {
                *category_sums.entry(entry.category_id.clone()).or_insert(0) += entry.amount;
            }

            match biggest {
                None => biggest = Some(entry),
                Some(b) if entry.amount > b.amount => biggest = Some(entry),
                _ => {}
            }

            if let Ok(date) = chrono::NaiveDate::parse_from_str(&entry.entry_date, "%Y-%m-%d") {
                let m = (date.month() as usize).saturating_sub(1);
                if m < 12 {
                    let kind = EntryKind::try_from(entry.kind).unwrap_or(EntryKind::Unspecified);
                    match kind {
                        EntryKind::Income => month_income_map[m] += entry.amount,
                        EntryKind::Expense => month_expense_map[m] += entry.amount,
                        _ => {}
                    }
                }
            }
        }

        let mut sorted: Vec<(String, i64)> = category_sums.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        let top_categories: Vec<CategoryTotal> = sorted
            .into_iter()
            .take(5)
            .map(|(category_id, total)| CategoryTotal {
                category_id,
                category_name: String::new(),
                total,
            })
            .collect();

        let biggest_transaction = biggest
            .map(|e| {
                let kind = EntryKind::try_from(e.kind).unwrap_or(EntryKind::Unspecified);
                let kind_str = match kind {
                    EntryKind::Income => "INCOME",
                    EntryKind::Expense => "EXPENSE",
                    _ => "UNKNOWN",
                };
                let entry_id = e.base.as_ref()
                    .map(|b| b.id.clone())
                    .unwrap_or_default();
                DrilledEntry {
                    entry_id,
                    entry_date: e.entry_date.clone(),
                    amount: e.amount,
                    description: e.description.clone(),
                    kind: kind_str.to_string(),
                }
            })
            .unwrap_or(DrilledEntry {
                entry_id: String::new(),
                entry_date: String::new(),
                amount: 0,
                description: String::new(),
                kind: "UNKNOWN".to_string(),
            });

        Ok(YearlySummaryResponse {
            top_categories,
            biggest_transaction: Some(biggest_transaction),
            month_income_map,
            month_expense_map,
        })
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}