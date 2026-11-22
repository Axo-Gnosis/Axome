use axome_protocol::memory::MemoryStatus;

use crate::{errors::ApiError, Host};

impl Host {
    pub fn memory_status(&self) -> Result<MemoryStatus, ApiError> {
        self.axo
            .block_on(self.memory.status(None))?
            .map_err(Into::into)
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
