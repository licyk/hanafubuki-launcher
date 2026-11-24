//! Racing the two update sources under the grace period.

use futures_util::future::{Either, select};

use super::{AppUpdateService, SOURCE_GRACE};
use crate::internal::hanafubuki_app_update::AppUpdateFuture;

impl AppUpdateService {
    /// Runs both source requests together. Once one answers successfully the
    /// other has [`SOURCE_GRACE`] to finish; a request still pending after that
    /// is dropped and reported through `expired`.
    pub(super) async fn settle_both<'a, T, E>(
        &self,
        left: AppUpdateFuture<'a, Result<T, E>>,
        right: AppUpdateFuture<'a, Result<T, E>>,
        expired: impl Fn() -> E,
    ) -> (Result<T, E>, Result<T, E>) {
        match select(left, right).await {
            Either::Left((first, pending)) => {
                let second = self.finish_pending(first.is_ok(), pending, expired).await;
                (first, second)
            }
            Either::Right((first, pending)) => {
                let second = self.finish_pending(first.is_ok(), pending, expired).await;
                (second, first)
            }
        }
    }

    async fn finish_pending<'a, T, E>(
        &self,
        other_answered: bool,
        pending: AppUpdateFuture<'a, Result<T, E>>,
        expired: impl Fn() -> E,
    ) -> Result<T, E> {
        if !other_answered {
            return pending.await;
        }
        match select(pending, self.timer.sleep(SOURCE_GRACE)).await {
            Either::Left((result, _)) => result,
            Either::Right(((), _)) => Err(expired()),
        }
    }
}

pub(super) fn grace_expired() -> String {
    format!(
        "no answer within {} s of the other source",
        SOURCE_GRACE.as_secs()
    )
}
