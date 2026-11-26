use axome_protocol::predict::{PredictStatus, PredictSuggestion};

use crate::{errors::ApiError, Host};

impl Host {
    pub fn predict_complete(
        &self,
        before: String,
        after: String,
    ) -> Result<Option<PredictSuggestion>, ApiError> {
        Ok(self.predict.complete(&before, &after))
    }

    pub fn predict_status(&self) -> Result<PredictStatus, ApiError> {
        Ok(self.predict.status())
    }

    pub fn predict_reset(&self) -> Result<(), ApiError> {
        self.predict.reset()
    }
}
