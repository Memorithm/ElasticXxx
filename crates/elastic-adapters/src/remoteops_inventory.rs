//! Strict ElasticXxx consumer boundary for RemoteOps host inventory schema v2.
//!
//! This wire contract preserves unknown observations and separates host totals
//! from cgroup limits. It does not infer free capacity, placement eligibility,
//! or enforcement from an inventory record.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Schema version emitted by remoteops-sandbox host resource inventory v2.
pub const REMOTEOPS_HOST_RESOURCE_INVENTORY_SCHEMA_V2: u32 = 2;

/// Explicit cgroup-limit state from RemoteOps.
///
/// Unknown and unbounded are deliberately separate. Consumers must not
/// reinterpret an unreadable controller file as an unlimited resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum RemoteOpsLimitObservationV2 {
    Unknown,
    Unbounded,
    Limited { value: u64 },
}

/// Unvalidated wire form emitted by the RemoteOps v2 inventory command.
///
/// Deserialization rejects unknown fields. Call [Self::into_validated] before
/// using any value; nullable keys are required to be present so a truncated
/// payload cannot silently become an unknown observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOpsHostResourceInventoryWireV2 {
    schema_version: u32,
    #[serde(deserialize_with = "deserialize_required_option")]
    cpu_logical_count: Option<usize>,
    #[serde(deserialize_with = "deserialize_required_option")]
    memory_total_bytes: Option<u64>,
    cgroup_cpu_quota_millis: RemoteOpsLimitObservationV2,
    cgroup_memory_limit_bytes: RemoteOpsLimitObservationV2,
}

/// Validated RemoteOps host inventory v2.
///
/// The values remain observations only; this type has no method that converts
/// them into available-capacity or backend-enforcement claims.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteOpsHostResourceInventoryV2 {
    cpu_logical_count: Option<usize>,
    memory_total_bytes: Option<u64>,
    cgroup_cpu_quota_millis: RemoteOpsLimitObservationV2,
    cgroup_memory_limit_bytes: RemoteOpsLimitObservationV2,
}

impl RemoteOpsHostResourceInventoryWireV2 {
    /// Revalidate the wire record and its schema version.
    ///
    /// # Errors
    ///
    /// Rejects unsupported versions and impossible zero host observations.
    pub fn into_validated(
        self,
    ) -> Result<RemoteOpsHostResourceInventoryV2, RemoteOpsInventoryContractError> {
        if self.schema_version != REMOTEOPS_HOST_RESOURCE_INVENTORY_SCHEMA_V2 {
            return Err(RemoteOpsInventoryContractError::UnsupportedSchemaVersion {
                observed: self.schema_version,
            });
        }
        if self.cpu_logical_count == Some(0) {
            return Err(RemoteOpsInventoryContractError::ZeroLogicalCpuCount);
        }
        if self.memory_total_bytes == Some(0) {
            return Err(RemoteOpsInventoryContractError::ZeroHostMemoryTotal);
        }

        Ok(RemoteOpsHostResourceInventoryV2 {
            cpu_logical_count: self.cpu_logical_count,
            memory_total_bytes: self.memory_total_bytes,
            cgroup_cpu_quota_millis: self.cgroup_cpu_quota_millis,
            cgroup_memory_limit_bytes: self.cgroup_memory_limit_bytes,
        })
    }
}

impl RemoteOpsHostResourceInventoryV2 {
    /// Host-visible logical CPU count, if observed.
    #[must_use]
    pub const fn cpu_logical_count(&self) -> Option<usize> {
        self.cpu_logical_count
    }

    /// Host total memory from /proc/meminfo, if observed.
    #[must_use]
    pub const fn memory_total_bytes(&self) -> Option<u64> {
        self.memory_total_bytes
    }

    /// Cgroup CPU quota, distinct from host-visible logical CPU count.
    #[must_use]
    pub const fn cgroup_cpu_quota_millis(&self) -> RemoteOpsLimitObservationV2 {
        self.cgroup_cpu_quota_millis
    }

    /// Cgroup memory limit, distinct from host total memory.
    #[must_use]
    pub const fn cgroup_memory_limit_bytes(&self) -> RemoteOpsLimitObservationV2 {
        self.cgroup_memory_limit_bytes
    }
}

/// Fail-closed semantic validation errors for RemoteOps inventory v2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteOpsInventoryContractError {
    UnsupportedSchemaVersion { observed: u32 },
    ZeroLogicalCpuCount,
    ZeroHostMemoryTotal,
}

impl fmt::Display for RemoteOpsInventoryContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { observed } => write!(
                formatter,
                "unsupported RemoteOps host inventory schema version {observed}"
            ),
            Self::ZeroLogicalCpuCount => {
                formatter.write_str("RemoteOps host inventory logical CPU count must be positive")
            }
            Self::ZeroHostMemoryTotal => {
                formatter.write_str("RemoteOps host inventory total memory must be positive")
            }
        }
    }
}

impl std::error::Error for RemoteOpsInventoryContractError {}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_INVENTORY: &str = r#"{
        "schema_version": 2,
        "cpu_logical_count": 4,
        "memory_total_bytes": 17179869184,
        "cgroup_cpu_quota_millis": {"state": "limited", "value": 2500},
        "cgroup_memory_limit_bytes": {"state": "unbounded"}
    }"#;

    #[test]
    fn validates_remoteops_v2_without_collapsing_host_and_cgroup_values() {
        let wire: RemoteOpsHostResourceInventoryWireV2 =
            serde_json::from_str(VALID_INVENTORY).expect("v2 wire record");
        let inventory = wire.into_validated().expect("valid v2 record");

        assert_eq!(inventory.cpu_logical_count(), Some(4));
        assert_eq!(inventory.memory_total_bytes(), Some(17_179_869_184));
        assert_eq!(
            inventory.cgroup_cpu_quota_millis(),
            RemoteOpsLimitObservationV2::Limited { value: 2500 }
        );
        assert_eq!(
            inventory.cgroup_memory_limit_bytes(),
            RemoteOpsLimitObservationV2::Unbounded
        );
    }

    #[test]
    fn unknown_and_unbounded_limits_remain_distinct() {
        let json = VALID_INVENTORY.replace(r#"{"state": "unbounded"}"#, r#"{"state": "unknown"}"#);
        let wire: RemoteOpsHostResourceInventoryWireV2 =
            serde_json::from_str(&json).expect("wire record");
        assert_eq!(
            wire.into_validated()
                .expect("valid record")
                .cgroup_memory_limit_bytes(),
            RemoteOpsLimitObservationV2::Unknown
        );
    }

    #[test]
    fn rejects_unknown_schema_and_unknown_fields() {
        let version_one =
            VALID_INVENTORY.replace(r#""schema_version": 2"#, r#""schema_version": 1"#);
        let wire: RemoteOpsHostResourceInventoryWireV2 =
            serde_json::from_str(&version_one).expect("wire shape");
        assert_eq!(
            wire.into_validated(),
            Err(RemoteOpsInventoryContractError::UnsupportedSchemaVersion { observed: 1 })
        );

        let unknown_root = VALID_INVENTORY.replace(
            r#""memory_total_bytes": 17179869184,"#,
            r#""memory_total_bytes": 17179869184, "future_field": true,"#,
        );
        assert!(
            serde_json::from_str::<RemoteOpsHostResourceInventoryWireV2>(&unknown_root).is_err()
        );

        let unknown_nested = VALID_INVENTORY.replace(
            r#"{"state": "unbounded"}"#,
            r#"{"state": "unbounded", "future_field": true}"#,
        );
        assert!(
            serde_json::from_str::<RemoteOpsHostResourceInventoryWireV2>(&unknown_nested).is_err()
        );
    }

    #[test]
    fn requires_nullable_fields_and_rejects_zero_host_totals() {
        let missing = VALID_INVENTORY.replace(r#""cpu_logical_count": 4,"#, "");
        assert!(serde_json::from_str::<RemoteOpsHostResourceInventoryWireV2>(&missing).is_err());

        let zero_cpu =
            VALID_INVENTORY.replace(r#""cpu_logical_count": 4"#, r#""cpu_logical_count": 0"#);
        let wire: RemoteOpsHostResourceInventoryWireV2 =
            serde_json::from_str(&zero_cpu).expect("wire shape");
        assert_eq!(
            wire.into_validated(),
            Err(RemoteOpsInventoryContractError::ZeroLogicalCpuCount)
        );

        let zero_memory = VALID_INVENTORY.replace(
            r#""memory_total_bytes": 17179869184"#,
            r#""memory_total_bytes": 0"#,
        );
        let wire: RemoteOpsHostResourceInventoryWireV2 =
            serde_json::from_str(&zero_memory).expect("wire shape");
        assert_eq!(
            wire.into_validated(),
            Err(RemoteOpsInventoryContractError::ZeroHostMemoryTotal)
        );
    }
}
