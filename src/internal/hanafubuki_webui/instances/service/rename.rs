use crate::internal::hanafubuki_webui::instances::{
    RenameWebUiInstanceRequest, WebUiInstanceSnapshot, identity::normalize_instance_name,
};

use super::WebUiInstanceService;

impl WebUiInstanceService {
    pub fn rename(
        &self,
        mut request: RenameWebUiInstanceRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        request.name = normalize_instance_name(&request.name)?;
        self.repository.rename(&request)?;
        self.get(&request.instance_id)
    }
}
