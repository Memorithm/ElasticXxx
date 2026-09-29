//! Bounded comparison of task preflight estimates with RemoteOps v2 cgroup ceilings.
//!
//! A value within a reported ceiling is not free capacity, a worker-placement
//! decision, or proof that the selected backend enforces the task's budget.
//! Inventory v2 also has no capture timestamp, so callers remain responsible
//! for freshness before using this diagnostic.

use std::fmt;

use elastic_eir::Fingerprint;

use crate::remoteops_inventory::{
    RemoteOpsHostResourceInventoryV2, RemoteOpsLimitObservationV2,
    REMOTEOPS_HOST_RESOURCE_INVENTORY_SCHEMA_V2,
};
use crate::task_resource_envelope::{TaskResourceEnvelopeV1, TaskResourcePlanV1};

/// Stable identity of the AXE-4 observed-cgroup-limit assessment.
pub const TASK_RESOURCE_HOST_LIMIT_ASSESSMENT_V1: &str =
    "elastic.task-resource-host-limit-assessment@1.0.0";

/// How one plan estimate compares with a RemoteOps v2 observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskResourceHostLimitStatusV1 {
    /// The plan does not request this dimension.
    NotRequested,
    /// The request does not exceed a numeric cgroup ceiling.
    WithinObservedCeiling,
    /// The request exceeds a numeric cgroup ceiling.
    ExceedsObservedCeiling,
    /// RemoteOps could not read the cgroup limit.
    Unknown,
    /// RemoteOps observed no cgroup ceiling; available capacity remains unknown.
    Unbounded,
    /// Inventory v2 does not report this resource dimension.
    UnsupportedByInventory,
}

/// Task resource dimension names included in an assessment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskResourceDimensionV1 {
    CpuMillis,
    MemoryBytes,
    GpuDevices,
    ModelTokens,
    EnergyMicrojoules,
    ThermalMarginMillicelsius,
}

/// One requested value and the corresponding source observation, when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskResourceHostLimitCheckV1 {
    requested: Option<u64>,
    observed_limit: Option<RemoteOpsLimitObservationV2>,
    status: TaskResourceHostLimitStatusV1,
}

impl TaskResourceHostLimitCheckV1 {
    /// Caller-supplied plan value, if present.
    #[must_use]
    pub const fn requested(&self) -> Option<u64> {
        self.requested
    }

    /// RemoteOps cgroup observation for CPU or memory.
    #[must_use]
    pub const fn observed_limit(&self) -> Option<RemoteOpsLimitObservationV2> {
        self.observed_limit
    }

    /// Comparison result; it is never an admission or enforcement decision.
    #[must_use]
    pub const fn status(&self) -> TaskResourceHostLimitStatusV1 {
        self.status
    }
}

/// Read-only comparison of a task plan with RemoteOps v2 cgroup ceilings.
///
/// The report deliberately has no overall compatible or admitted state.
/// Unknown and unbounded observations remain distinct, and v2 cannot assess
/// GPU, tokens, energy, or thermal margin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskResourceHostLimitAssessmentV1 {
    plan: TaskResourcePlanV1,
    inventory_fingerprint: Fingerprint,
    cpu_millis: TaskResourceHostLimitCheckV1,
    memory_bytes: TaskResourceHostLimitCheckV1,
    gpu_devices: TaskResourceHostLimitCheckV1,
    model_tokens: TaskResourceHostLimitCheckV1,
    energy_microjoules: TaskResourceHostLimitCheckV1,
    thermal_margin_millicelsius: TaskResourceHostLimitCheckV1,
}

impl TaskResourceHostLimitAssessmentV1 {
    /// Whether this diagnostic remains bound to the same plan envelope and
    /// the same CPU/memory observations from RemoteOps inventory v2.
    #[must_use]
    pub fn is_bound_to(
        &self,
        envelope: &TaskResourceEnvelopeV1,
        inventory: &RemoteOpsHostResourceInventoryV2,
    ) -> bool {
        self.plan.is_bound_to(envelope)
            && self.inventory_fingerprint == inventory_fingerprint(inventory)
    }

    /// Plan values used by the comparison.
    #[must_use]
    pub const fn plan(&self) -> &TaskResourcePlanV1 {
        &self.plan
    }

    #[must_use]
    pub const fn cpu_millis(&self) -> TaskResourceHostLimitCheckV1 {
        self.cpu_millis
    }

    #[must_use]
    pub const fn memory_bytes(&self) -> TaskResourceHostLimitCheckV1 {
        self.memory_bytes
    }

    #[must_use]
    pub const fn gpu_devices(&self) -> TaskResourceHostLimitCheckV1 {
        self.gpu_devices
    }

    #[must_use]
    pub const fn model_tokens(&self) -> TaskResourceHostLimitCheckV1 {
        self.model_tokens
    }

    #[must_use]
    pub const fn energy_microjoules(&self) -> TaskResourceHostLimitCheckV1 {
        self.energy_microjoules
    }

    #[must_use]
    pub const fn thermal_margin_millicelsius(&self) -> TaskResourceHostLimitCheckV1 {
        self.thermal_margin_millicelsius
    }

    /// Returns dimensions that inventory v2 cannot compare.
    #[must_use]
    pub fn unsupported_dimensions(&self) -> Vec<TaskResourceDimensionV1> {
        [
            (TaskResourceDimensionV1::GpuDevices, self.gpu_devices.status),
            (
                TaskResourceDimensionV1::ModelTokens,
                self.model_tokens.status,
            ),
            (
                TaskResourceDimensionV1::EnergyMicrojoules,
                self.energy_microjoules.status,
            ),
            (
                TaskResourceDimensionV1::ThermalMarginMillicelsius,
                self.thermal_margin_millicelsius.status,
            ),
        ]
        .into_iter()
        .filter_map(|(dimension, status)| {
            (status == TaskResourceHostLimitStatusV1::UnsupportedByInventory).then_some(dimension)
        })
        .collect()
    }

    /// Whether any numeric CPU or memory ceiling is known to be exceeded.
    ///
    /// A false result does not mean the task has sufficient available capacity
    /// or that the backend can enforce the request.
    #[must_use]
    pub const fn has_observed_exceedance(&self) -> bool {
        matches!(
            self.cpu_millis.status,
            TaskResourceHostLimitStatusV1::ExceedsObservedCeiling
        ) || matches!(
            self.memory_bytes.status,
            TaskResourceHostLimitStatusV1::ExceedsObservedCeiling
        )
    }
}

/// Fail-closed errors for the AXE-4 comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskResourceHostLimitErrorV1 {
    PlanNotBoundToEnvelope,
}

impl fmt::Display for TaskResourceHostLimitErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlanNotBoundToEnvelope => {
                formatter.write_str("task resource plan is not bound to this envelope")
            }
        }
    }
}

impl std::error::Error for TaskResourceHostLimitErrorV1 {}

/// Compare CPU and memory plan estimates with RemoteOps v2 cgroup ceilings.
///
/// The operation is diagnostic only. A numeric exceedance proves the estimate
/// is above that observed ceiling; a value below it does not establish free
/// capacity, placement compatibility, freshness, or backend enforcement.
pub fn assess_task_resource_plan_against_remoteops_v2(
    envelope: &TaskResourceEnvelopeV1,
    plan: &TaskResourcePlanV1,
    inventory: &RemoteOpsHostResourceInventoryV2,
) -> Result<TaskResourceHostLimitAssessmentV1, TaskResourceHostLimitErrorV1> {
    if !plan.is_bound_to(envelope) {
        return Err(TaskResourceHostLimitErrorV1::PlanNotBoundToEnvelope);
    }

    let gpu_devices = plan.gpu_devices().map(u64::from);
    let gpu_check = match gpu_devices {
        Some(0) | None => not_requested(gpu_devices),
        Some(_) => unsupported(gpu_devices),
    };

    Ok(TaskResourceHostLimitAssessmentV1 {
        plan: plan.clone(),
        inventory_fingerprint: inventory_fingerprint(inventory),
        cpu_millis: compare_limit(plan.cpu_millis(), inventory.cgroup_cpu_quota_millis()),
        memory_bytes: compare_limit(plan.memory_bytes(), inventory.cgroup_memory_limit_bytes()),
        gpu_devices: gpu_check,
        model_tokens: unsupported(plan.model_tokens()),
        energy_microjoules: unsupported(plan.energy_microjoules()),
        thermal_margin_millicelsius: unsupported(plan.thermal_margin_millicelsius()),
    })
}

fn compare_limit(
    requested: Option<u64>,
    observed_limit: RemoteOpsLimitObservationV2,
) -> TaskResourceHostLimitCheckV1 {
    let status = match (requested, observed_limit) {
        (None, _) => TaskResourceHostLimitStatusV1::NotRequested,
        (Some(_), RemoteOpsLimitObservationV2::Unknown) => TaskResourceHostLimitStatusV1::Unknown,
        (Some(_), RemoteOpsLimitObservationV2::Unbounded) => {
            TaskResourceHostLimitStatusV1::Unbounded
        }
        (Some(requested), RemoteOpsLimitObservationV2::Limited { value }) if requested <= value => {
            TaskResourceHostLimitStatusV1::WithinObservedCeiling
        }
        (Some(_), RemoteOpsLimitObservationV2::Limited { .. }) => {
            TaskResourceHostLimitStatusV1::ExceedsObservedCeiling
        }
    };

    TaskResourceHostLimitCheckV1 {
        requested,
        observed_limit: Some(observed_limit),
        status,
    }
}

fn unsupported(requested: Option<u64>) -> TaskResourceHostLimitCheckV1 {
    TaskResourceHostLimitCheckV1 {
        requested,
        observed_limit: None,
        status: if requested.is_some() {
            TaskResourceHostLimitStatusV1::UnsupportedByInventory
        } else {
            TaskResourceHostLimitStatusV1::NotRequested
        },
    }
}

fn not_requested(requested: Option<u64>) -> TaskResourceHostLimitCheckV1 {
    TaskResourceHostLimitCheckV1 {
        requested,
        observed_limit: None,
        status: TaskResourceHostLimitStatusV1::NotRequested,
    }
}

fn inventory_fingerprint(inventory: &RemoteOpsHostResourceInventoryV2) -> Fingerprint {
    let fingerprint = Fingerprint::EMPTY
        .text(TASK_RESOURCE_HOST_LIMIT_ASSESSMENT_V1)
        .number(u64::from(REMOTEOPS_HOST_RESOURCE_INVENTORY_SCHEMA_V2));
    let fingerprint = fingerprint_limit(
        fingerprint.text("cgroup_cpu_quota_millis"),
        inventory.cgroup_cpu_quota_millis(),
    );
    fingerprint_limit(
        fingerprint.text("cgroup_memory_limit_bytes"),
        inventory.cgroup_memory_limit_bytes(),
    )
}

fn fingerprint_limit(
    fingerprint: Fingerprint,
    observation: RemoteOpsLimitObservationV2,
) -> Fingerprint {
    match observation {
        RemoteOpsLimitObservationV2::Unknown => fingerprint.number(0),
        RemoteOpsLimitObservationV2::Unbounded => fingerprint.number(1),
        RemoteOpsLimitObservationV2::Limited { value } => fingerprint.number(2).number(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remoteops_inventory::RemoteOpsHostResourceInventoryWireV2;
    use crate::task_resource_envelope::{TaskResourceBudgetV1, TaskResourcePlanEstimateV1};

    const INVENTORY_TEMPLATE: &str = r#"{
        "schema_version": 2,
        "cpu_logical_count": 8,
        "memory_total_bytes": 17179869184,
        "cgroup_cpu_quota_millis": __CPU__,
        "cgroup_memory_limit_bytes": __MEMORY__
    }"#;

    fn inventory(cpu: &str, memory: &str) -> RemoteOpsHostResourceInventoryV2 {
        let json = INVENTORY_TEMPLATE
            .replace("__CPU__", cpu)
            .replace("__MEMORY__", memory);
        serde_json::from_str::<RemoteOpsHostResourceInventoryWireV2>(&json)
            .expect("RemoteOps v2 wire record")
            .into_validated()
            .expect("valid inventory")
    }

    fn envelope(cpu_millis: Option<u64>, memory_bytes: Option<u64>) -> TaskResourceEnvelopeV1 {
        TaskResourceEnvelopeV1::new(
            "task:host-limit",
            "workspace:host-limit",
            TaskResourceBudgetV1::new(cpu_millis, memory_bytes, None, None, None, None)
                .expect("valid budget"),
        )
        .expect("valid envelope")
    }

    #[test]
    fn compares_only_against_numeric_cgroup_ceilings() {
        let envelope = envelope(Some(2_000), Some(2 * 1024 * 1024 * 1024));
        let plan = envelope
            .preflight_plan(Some(1_500), Some(512 * 1024 * 1024), None, None)
            .expect("valid plan");
        let inventory = inventory(
            r#"{"state":"limited","value":2500}"#,
            r#"{"state":"limited","value":1073741824}"#,
        );

        let assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &inventory)
                .expect("assessment");
        assert_eq!(
            assessment.cpu_millis().status(),
            TaskResourceHostLimitStatusV1::WithinObservedCeiling
        );
        assert_eq!(
            assessment.memory_bytes().status(),
            TaskResourceHostLimitStatusV1::WithinObservedCeiling
        );
        assert!(!assessment.has_observed_exceedance());
        assert!(assessment.is_bound_to(&envelope, &inventory));
    }

    #[test]
    fn reports_known_exceedance_without_calling_it_capacity_or_admission() {
        let envelope = envelope(Some(2_000), Some(2 * 1024 * 1024 * 1024));
        let plan = envelope
            .preflight_plan(Some(1_500), Some(1536 * 1024 * 1024), None, None)
            .expect("valid plan against task envelope");
        let inventory = inventory(
            r#"{"state":"limited","value":2500}"#,
            r#"{"state":"limited","value":1073741824}"#,
        );

        let assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &inventory)
                .expect("assessment");
        assert_eq!(
            assessment.memory_bytes().status(),
            TaskResourceHostLimitStatusV1::ExceedsObservedCeiling
        );
        assert!(assessment.has_observed_exceedance());
    }

    #[test]
    fn unknown_and_unbounded_observations_remain_distinct() {
        let envelope = envelope(Some(2_000), None);
        let plan = envelope
            .preflight_plan(Some(1_500), None, None, None)
            .expect("valid plan");
        let unknown = inventory(r#"{"state":"unknown"}"#, r#"{"state":"unknown"}"#);
        let unbounded = inventory(r#"{"state":"unbounded"}"#, r#"{"state":"unbounded"}"#);

        let unknown_assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &unknown)
                .expect("unknown assessment");
        let unbounded_assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &unbounded)
                .expect("unbounded assessment");
        assert_eq!(
            unknown_assessment.cpu_millis().status(),
            TaskResourceHostLimitStatusV1::Unknown
        );
        assert_eq!(
            unbounded_assessment.cpu_millis().status(),
            TaskResourceHostLimitStatusV1::Unbounded
        );
        assert!(!unknown_assessment.is_bound_to(&envelope, &unbounded));
    }

    #[test]
    fn dimensions_absent_from_inventory_v2_are_explicitly_unsupported() {
        let budget = TaskResourceBudgetV1::new(None, None, None, Some(2), Some(1_000), None)
            .expect("valid budget")
            .with_max_energy_microjoules(Some(4_000_000))
            .expect("valid energy bound")
            .with_minimum_thermal_margin_millicelsius(Some(750));
        let envelope = TaskResourceEnvelopeV1::new("task:extended", "workspace:extended", budget)
            .expect("envelope");
        let plan = envelope
            .preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                gpu_devices: Some(1),
                model_tokens: Some(800),
                energy_microjoules: Some(3_000_000),
                thermal_margin_millicelsius: Some(1_000),
                ..TaskResourcePlanEstimateV1::default()
            })
            .expect("valid extended plan");
        let inventory = inventory(r#"{"state":"unbounded"}"#, r#"{"state":"unbounded"}"#);

        let assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &inventory)
                .expect("assessment");
        assert_eq!(
            assessment.unsupported_dimensions(),
            vec![
                TaskResourceDimensionV1::GpuDevices,
                TaskResourceDimensionV1::ModelTokens,
                TaskResourceDimensionV1::EnergyMicrojoules,
                TaskResourceDimensionV1::ThermalMarginMillicelsius,
            ]
        );
    }

    #[test]
    fn cpu_only_zero_gpu_request_needs_no_gpu_observation() {
        let budget = TaskResourceBudgetV1::new(None, None, None, Some(0), None, None)
            .expect("CPU-only budget");
        let envelope = TaskResourceEnvelopeV1::new("task:cpu-only", "workspace:cpu-only", budget)
            .expect("envelope");
        let plan = envelope
            .preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                gpu_devices: Some(0),
                ..TaskResourcePlanEstimateV1::default()
            })
            .expect("CPU-only plan");
        let inventory = inventory(r#"{"state":"unbounded"}"#, r#"{"state":"unbounded"}"#);

        let assessment =
            assess_task_resource_plan_against_remoteops_v2(&envelope, &plan, &inventory)
                .expect("assessment");
        assert_eq!(
            assessment.gpu_devices().status(),
            TaskResourceHostLimitStatusV1::NotRequested
        );
        assert!(assessment.unsupported_dimensions().is_empty());
    }

    #[test]
    fn rejects_a_plan_from_another_envelope() {
        let first = envelope(Some(2_000), Some(1024));
        let second = TaskResourceEnvelopeV1::new(
            "task:other",
            "workspace:host-limit",
            TaskResourceBudgetV1::new(Some(2_000), Some(1024), None, None, None, None)
                .expect("valid budget"),
        )
        .expect("valid envelope");
        let plan = first
            .preflight_plan(Some(1_500), Some(512), None, None)
            .expect("valid plan");
        let inventory = inventory(
            r#"{"state":"limited","value":2500}"#,
            r#"{"state":"limited","value":1073741824}"#,
        );

        assert_eq!(
            assess_task_resource_plan_against_remoteops_v2(&second, &plan, &inventory),
            Err(TaskResourceHostLimitErrorV1::PlanNotBoundToEnvelope)
        );
    }
}
