use super::WebUiInstanceLifecycle;

pub(super) fn validate_transition(
    current: &WebUiInstanceLifecycle,
    next: &WebUiInstanceLifecycle,
) -> Result<(), String> {
    let valid = match (current, next) {
        (WebUiInstanceLifecycle::Created, WebUiInstanceLifecycle::Installing { .. }) => true,
        (WebUiInstanceLifecycle::Created, WebUiInstanceLifecycle::Deleting)
        | (WebUiInstanceLifecycle::Ready { .. }, WebUiInstanceLifecycle::Deleting)
        | (WebUiInstanceLifecycle::InstallFailed { .. }, WebUiInstanceLifecycle::Deleting)
        | (WebUiInstanceLifecycle::Deleting, WebUiInstanceLifecycle::Deleting) => true,
        (
            WebUiInstanceLifecycle::InstallFailed { .. },
            WebUiInstanceLifecycle::Installing { .. },
        ) => true,
        (
            WebUiInstanceLifecycle::Installing {
                operation_id: current,
            },
            WebUiInstanceLifecycle::Ready { operation_id }
            | WebUiInstanceLifecycle::InstallFailed { operation_id, .. },
        ) => current == operation_id,
        (current, next) => current == next,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid WebUI instance lifecycle transition: {current:?} -> {next:?}"
        ))
    }
}
