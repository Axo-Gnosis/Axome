use std::path::Path;

use axome_protocol::memory::{MemoryCaptureQuery, MemoryCaptureRun, MemoryStatus};

use crate::{errors::ApiError, Host};

impl Host {
    pub fn memory_status(&self) -> Result<MemoryStatus, ApiError> {
        self.axo
            .block_on(self.memory.status(None))?
            .map_err(Into::into)
    }

    pub fn memory_captures(
        &self,
        query: MemoryCaptureQuery,
    ) -> Result<Vec<MemoryCaptureRun>, ApiError> {
        crate::services::memory::captures::list(&self.db, &query).map_err(Into::into)
    }

    pub fn memory_capture_revert(&self, fact_id: i64) -> Result<MemoryCaptureRun, ApiError> {
        self.axo
            .block_on(self.memory.revert_fact(fact_id))?
            .map_err(Into::into)
    }

    pub fn memory_folder(&self, scope_folder: String) -> Result<Option<String>, ApiError> {
        Ok(self
            .memory
            .active(Path::new(&scope_folder))
            .map(|fm| fm.bank.name))
    }

    pub fn memory_retry_download(&self) -> Result<(), ApiError> {
        self.memory.refresh(true);
        Ok(())
    }

    pub fn memory_ensure_bank(&self, name: String) -> Result<String, ApiError> {
        let path = self.axo.block_on(self.memory.ensure_bank(name))??;
        Ok(path.to_string_lossy().into_owned())
    }
}
