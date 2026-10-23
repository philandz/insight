use crate::pb::service::budget::budget_service_client::BudgetServiceClient;
use crate::pb::service::budget::{
    BudgetRole, CheckRoleRequest, GetRolloverPolicyRequest, ListBudgetsRequest, RolloverPolicy,
};
use crate::pb::service::category::category_service_client::CategoryServiceClient;
use crate::pb::service::category::{BudgetPlan, GetPlanRangeRequest};
use crate::pb::service::entry::{PeriodTotalsRequest, PeriodTotalsRow};
use crate::pb::service::entry::entry_service_client::EntryServiceClient;
use crate::pb::service::entry::ListParams;
use tonic::transport::Channel;

pub struct EntryClient {
    inner: EntryServiceClient<Channel>,
}

impl EntryClient {
    pub async fn connect(url: &str) -> Result<Self, tonic::transport::Error> {
        let channel = Channel::from_shared(url.to_string())
            .expect("invalid entry gRPC URL")
            .connect()
            .await?;
        Ok(Self {
            inner: EntryServiceClient::new(channel),
        })
    }

    pub async fn list_entries(
        &mut self,
        user_id: &str,
        budget_id: &str,
        date_from: &str,
        date_to: &str,
        category_id: Option<&str>,
        page_size: i32,
    ) -> Result<crate::pb::service::entry::ListEntriesResponse, tonic::Status> {
        let params = ListParams {
            date_from: Some(date_from.to_string()),
            date_to: Some(date_to.to_string()),
            category_id: category_id.map(|s| s.to_string()),
            page_size: Some(page_size),
            ..Default::default()
        };

        let mut req = tonic::Request::new(crate::pb::service::entry::ListEntriesRequest {
            budget_id: Some(budget_id.to_string()),
            params: Some(params),
            budget_ids: vec![],
        });
        req.metadata_mut().insert(
            "x-user-id",
            tonic::metadata::MetadataValue::try_from(user_id)
                .map_err(|e| tonic::Status::internal(format!("invalid x-user-id: {}", e)))?,
        );

        let resp = self.inner.list_entries(req).await?;

        Ok(resp.into_inner())
    }
}

pub struct BudgetClient {
    inner: BudgetServiceClient<Channel>,
}

impl BudgetClient {
    pub async fn connect(url: &str) -> Result<Self, tonic::transport::Error> {
        let channel = Channel::from_shared(url.to_string())
            .expect("invalid budget gRPC URL")
            .connect()
            .await?;
        Ok(Self {
            inner: BudgetServiceClient::new(channel),
        })
    }

    pub async fn list_budgets(&mut self, org_id: &str, user_id: &str) -> Result<crate::pb::service::budget::ListBudgetsResponse, tonic::Status> {
        let mut req = tonic::Request::new(ListBudgetsRequest {
            org_id: org_id.to_string(),
            params: None,
        });
        req.metadata_mut().insert(
            "x-user-id",
            tonic::metadata::MetadataValue::try_from(user_id)
                .map_err(|e| tonic::Status::internal(format!("invalid x-user-id: {}", e)))?,
        );
        let resp = self.inner.list_budgets(req).await?;
        Ok(resp.into_inner())
    }

    /// Role and period settings of a budget for `user_id` (non-members get `Unspecified`).
    pub async fn check_role_ctx(
        &mut self,
        user_id: &str,
        budget_id: &str,
    ) -> Result<(BudgetRole, u8, String), tonic::Status> {
        let mut req = tonic::Request::new(CheckRoleRequest {
            user_id: user_id.to_string(),
            budget_id: budget_id.to_string(),
        });
        set_user(&mut req, user_id)?;
        let r = self.inner.check_role(req).await?.into_inner();
        let tz = if r.timezone.is_empty() {
            "Asia/Ho_Chi_Minh".to_string()
        } else {
            r.timezone
        };
        Ok((
            BudgetRole::try_from(r.role).unwrap_or(BudgetRole::Unspecified),
            r.period_start_day.clamp(1, 28) as u8,
            tz,
        ))
    }

    pub async fn get_rollover_policy(
        &mut self,
        user_id: &str,
        budget_id: &str,
    ) -> Result<RolloverPolicy, tonic::Status> {
        let mut req = tonic::Request::new(GetRolloverPolicyRequest {
            budget_id: budget_id.to_string(),
        });
        set_user(&mut req, user_id)?;
        let r = self.inner.get_rollover_policy(req).await?.into_inner();
        Ok(RolloverPolicy::try_from(r.policy).unwrap_or(RolloverPolicy::Reset))
    }
}

fn set_user<T>(req: &mut tonic::Request<T>, user_id: &str) -> Result<(), tonic::Status> {
    req.metadata_mut().insert(
        "x-user-id",
        tonic::metadata::MetadataValue::try_from(user_id)
            .map_err(|e| tonic::Status::internal(format!("invalid x-user-id: {}", e)))?,
    );
    Ok(())
}

impl EntryClient {
    /// Totals grouped by (category, day) over `[date_from, date_to)`.
    pub async fn period_totals_daily(
        &mut self,
        budget_id: &str,
        date_from: &str,
        date_to: &str,
    ) -> Result<Vec<PeriodTotalsRow>, tonic::Status> {
        let resp = self
            .inner
            .get_period_totals(PeriodTotalsRequest {
                budget_ids: vec![budget_id.to_string()],
                date_from: date_from.to_string(),
                date_to: date_to.to_string(),
                group_by_category: true,
                group_by_day: true,
            })
            .await?;
        Ok(resp.into_inner().rows)
    }
}

pub struct CategoryClient {
    inner: CategoryServiceClient<Channel>,
}

impl CategoryClient {
    /// Lazy: category and budget call each other, so start order must not matter.
    pub fn connect_lazy(url: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(url.to_string())
            .map_err(|e| anyhow::anyhow!("invalid category gRPC URL: {e}"))?
            .connect_lazy();
        Ok(Self {
            inner: CategoryServiceClient::new(channel),
        })
    }

    pub async fn plan_range(
        &mut self,
        user_id: &str,
        budget_id: &str,
        from_offset: i32,
        to_offset: i32,
    ) -> Result<Vec<BudgetPlan>, tonic::Status> {
        let mut req = tonic::Request::new(GetPlanRangeRequest {
            budget_id: budget_id.to_string(),
            from_offset,
            to_offset,
        });
        set_user(&mut req, user_id)?;
        Ok(self.inner.get_plan_range(req).await?.into_inner().plans)
    }
}
