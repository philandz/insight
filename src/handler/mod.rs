use crate::pb::service::insight::insight_service_server::InsightService;
use crate::pb::service::insight::{
    CategoryDrilldownRequest, CategoryDrilldownResponse, MonthlySummaryRequest,
    MonthlySummaryResponse, NetWorthRequest, NetWorthResponse, WeeklySummaryRequest,
    WeeklySummaryResponse, YearlySummaryRequest, YearlySummaryResponse,
};
use crate::manager::biz::InsightBiz;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct InsightHandler {
    biz: Arc<InsightBiz>,
}

impl InsightHandler {
    pub fn new(biz: Arc<InsightBiz>) -> Self {
        Self { biz }
    }
}

#[tonic::async_trait]
impl InsightService for InsightHandler {
    async fn get_monthly_summary(
        &self,
        request: Request<MonthlySummaryRequest>,
    ) -> Result<Response<MonthlySummaryResponse>, Status> {
        let user_id = crate::manager::validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self.biz.get_monthly_summary(&user_id, req).await?;
        Ok(Response::new(resp))
    }

    async fn get_weekly_summary(
        &self,
        request: Request<WeeklySummaryRequest>,
    ) -> Result<Response<WeeklySummaryResponse>, Status> {
        let user_id = crate::manager::validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self.biz.get_weekly_summary(&user_id, req).await?;
        Ok(Response::new(resp))
    }

    async fn get_net_worth(
        &self,
        request: Request<NetWorthRequest>,
    ) -> Result<Response<NetWorthResponse>, Status> {
        let user_id = crate::manager::validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self.biz.get_net_worth(&user_id, req).await?;
        Ok(Response::new(resp))
    }

    async fn get_category_drilldown(
        &self,
        request: Request<CategoryDrilldownRequest>,
    ) -> Result<Response<CategoryDrilldownResponse>, Status> {
        let user_id = crate::manager::validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self.biz.get_category_drilldown(&user_id, req).await?;
        Ok(Response::new(resp))
    }

    async fn get_yearly_summary(
        &self,
        request: Request<YearlySummaryRequest>,
    ) -> Result<Response<YearlySummaryResponse>, Status> {
        let user_id = crate::manager::validate::user_id_from_metadata(request.metadata())?;
        let req = request.into_inner();
        let resp = self.biz.get_yearly_summary(&user_id, req).await?;
        Ok(Response::new(resp))
    }
}