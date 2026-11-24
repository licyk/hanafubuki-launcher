use super::super::{UpdateWebUiInstanceQuickPathsRequest, WebUiInstanceSnapshot};
use super::WebUiInstanceService;

impl WebUiInstanceService {
    /// Replace the instance's quick-access list.
    ///
    /// Quick paths name directories the user wants to reach quickly; they are
    /// not component locations, so no installation layout is validated and no
    /// lifecycle phase is blocked. The observed config revision is the only
    /// concurrency guard.
    pub fn update_quick_paths(
        &self,
        request: &UpdateWebUiInstanceQuickPathsRequest,
    ) -> Result<WebUiInstanceSnapshot, String> {
        self.repository.update_quick_paths(request)?;
        self.get(&request.instance_id)
    }
}
