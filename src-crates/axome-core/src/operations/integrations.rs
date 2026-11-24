use crate::{errors::ApiError, services::integrations::IntegrationService, Host};
use axome_protocol::integrations::{IntegrationCatalog, IntegrationGrammar};

impl Host {
    pub fn integrations_get_catalog(&self) -> Result<IntegrationCatalog, ApiError> {
        Ok(IntegrationService::current().catalog.clone())
    }

    pub fn integrations_get_grammars(&self) -> Result<Vec<IntegrationGrammar>, ApiError> {
        Ok(IntegrationService::current().grammars.clone())
    }

    pub fn integrations_reload(&self) -> Result<(), ApiError> {
        IntegrationService::reload();
        Ok(())
    }
}
