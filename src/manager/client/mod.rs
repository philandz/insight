use crate::pb::service::budget::budget_service_client::BudgetServiceClient;
use crate::pb::service::budget::ListBudgetsRequest;
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
        });
        req.metadata_mut().insert(
            "x-user-id",
            tonic::metadata::MetadataValue::try_from(user_id)
                .map_err(|e| tonic::Status::internal(format!("invalid x-user-id: {}", e)))?,
        );
        let resp = self.inner.list_budgets(req).await?;
        Ok(resp.into_inner())
    }
}