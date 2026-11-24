use std::collections::HashMap;

use crate::internal::hanafubuki_process::ownership::{
    ProcessOwnerId, RegistryReservation, ReservationSet, ReservedProcessReceipt,
};

use super::OwnedTaskRecord;

/// Shared identity/metadata infrastructure. App and Instance tasks keep
/// separate registry values and locks; only the common mechanics are shared.
pub(crate) struct OwnedTaskRegistry<Scope, Metadata> {
    records: HashMap<String, OwnedTaskRecord<Scope, Metadata>>,
    reservations: ReservationSet,
    #[cfg(any(test, feature = "test-support"))]
    reservation_count: usize,
}

impl<Scope, Metadata> Default for OwnedTaskRegistry<Scope, Metadata> {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            reservations: ReservationSet::default(),
            #[cfg(any(test, feature = "test-support"))]
            reservation_count: 0,
        }
    }
}

impl<Scope, Metadata> OwnedTaskRegistry<Scope, Metadata> {
    pub fn reserve(
        &mut self,
        reservation_key: &str,
        executor_id: &str,
        owner: &ProcessOwnerId,
    ) -> Result<RegistryReservation, String> {
        if self.records.contains_key(executor_id) {
            return Err(format!("task process already exists: {executor_id}"));
        }
        let reservation = self.reservations.reserve(reservation_key, owner)?;
        #[cfg(any(test, feature = "test-support"))]
        {
            self.reservation_count += 1;
        }
        Ok(reservation)
    }

    pub fn reservation_matches(&self, reservation: &RegistryReservation) -> bool {
        self.reservations.matches(reservation)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn finalize_reservation(
        &mut self,
        key: &str,
        owner: &ProcessOwnerId,
    ) -> crate::internal::hanafubuki_process::ownership::OwnedReleaseStatus {
        let status = self.reservations.release_owned(key, owner);
        if status == crate::internal::hanafubuki_process::ownership::OwnedReleaseStatus::Released {
            self.reservation_count = self.reservation_count.saturating_sub(1);
        }
        status
    }

    pub fn finalize_reservation_handle(
        &mut self,
        reservation: &RegistryReservation,
    ) -> crate::internal::hanafubuki_process::ownership::OwnedReleaseStatus {
        let status = self
            .reservations
            .release_owned(reservation.key(), reservation.reservation_id());
        #[cfg(any(test, feature = "test-support"))]
        if status == crate::internal::hanafubuki_process::ownership::OwnedReleaseStatus::Released {
            self.reservation_count = self.reservation_count.saturating_sub(1);
        }
        status
    }

    pub fn contains(&self, executor_id: &str) -> bool {
        self.records.contains_key(executor_id)
    }

    pub fn commit_reserved(
        &mut self,
        reservation: &RegistryReservation,
        executor_id: String,
        scope: Scope,
        metadata: Metadata,
        process: ReservedProcessReceipt,
    ) -> Result<(), String> {
        if !self.reservations.consume(reservation) {
            return Err(format!(
                "task reservation became stale: {}",
                reservation.key()
            ));
        }
        #[cfg(any(test, feature = "test-support"))]
        {
            self.reservation_count = self.reservation_count.saturating_sub(1);
        }
        self.records.insert(
            executor_id,
            OwnedTaskRecord {
                scope,
                metadata,
                process,
            },
        );
        Ok(())
    }

    pub fn get(&self, executor_id: &str) -> Option<&OwnedTaskRecord<Scope, Metadata>> {
        self.records.get(executor_id)
    }

    pub fn values(&self) -> impl Iterator<Item = &OwnedTaskRecord<Scope, Metadata>> {
        self.records.values()
    }

    pub fn process_matches(&self, receipt: &ReservedProcessReceipt) -> bool {
        self.records
            .get(receipt.executor_id())
            .is_some_and(|record| &record.process == receipt)
    }

    pub fn remove_matching(
        &mut self,
        receipt: &ReservedProcessReceipt,
    ) -> Option<OwnedTaskRecord<Scope, Metadata>> {
        if self.process_matches(receipt) {
            self.records.remove(receipt.executor_id())
        } else {
            None
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn process(&self, executor_id: &str) -> Option<ReservedProcessReceipt> {
        self.records
            .get(executor_id)
            .map(|record| record.process.clone())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn counts(&self) -> (usize, usize) {
        (self.records.len(), self.reservation_count)
    }
}
