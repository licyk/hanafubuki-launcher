use crate::internal::hanafubuki_process::ownership::ReservedProcessReceipt;

#[derive(Clone, Debug)]
pub(crate) struct OwnedTaskRecord<Scope, Metadata> {
    pub scope: Scope,
    pub metadata: Metadata,
    pub process: ReservedProcessReceipt,
}
