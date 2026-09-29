//! Native AXE-1 task resource envelope over opaque Hub-owned references.
//!
//! This adapter keeps task identity and workspace provenance caller-owned while
//! giving ElasticXxx a validated resource-boundary input. It neither discovers
//! host capacity nor authorizes physical actuation.

use elastic_eir::Fingerprint;
use std::fmt;

/// Stable identity of the native task resource envelope contract.
pub const TASK_RESOURCE_ENVELOPE_V1: &str = "elastic.task-resource-envelope@1.0.0";
/// Maximum byte length for opaque task and workspace reference labels.
pub const MAX_TASK_RESOURCE_REF_BYTES: usize = 256;

/// Optional hard resource bounds supplied by the task owner.
///
/// An absent dimension is unspecified, not unlimited. GPU count zero is
/// permitted to express an explicit CPU-only request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskResourceBudgetV1 {
    cpu_millis: Option<u64>,
    memory_bytes: Option<u64>,
    wall_clock_ms: Option<u64>,
    gpu_devices: Option<u32>,
    model_tokens: Option<u64>,
    concurrency: Option<u32>,
}

impl TaskResourceBudgetV1 {
    /// Construct and validate one resource budget.
    ///
    /// # Errors
    ///
    /// Rejects a zero CPU, memory, wall-clock, model-token, or concurrency
    /// bound. Zero GPU devices remains valid as a CPU-only requirement.
    pub fn new(
        cpu_millis: Option<u64>,
        memory_bytes: Option<u64>,
        wall_clock_ms: Option<u64>,
        gpu_devices: Option<u32>,
        model_tokens: Option<u64>,
        concurrency: Option<u32>,
    ) -> Result<Self, TaskResourceEnvelopeError> {
        for (dimension, value) in [
            ("cpu_millis", cpu_millis),
            ("memory_bytes", memory_bytes),
            ("wall_clock_ms", wall_clock_ms),
            ("model_tokens", model_tokens),
        ] {
            if value == Some(0) {
                return Err(TaskResourceEnvelopeError::ZeroBound { dimension });
            }
        }
        if gpu_devices.is_some_and(|value| value > 0) && gpu_devices == Some(0) {
            return Err(TaskResourceEnvelopeError::ZeroBound {
                dimension: "gpu_devices",
            });
        }
        if concurrency == Some(0) {
            return Err(TaskResourceEnvelopeError::ZeroBound {
                dimension: "concurrency",
            });
        }

        Ok(Self {
            cpu_millis,
            memory_bytes,
            wall_clock_ms,
            gpu_devices,
            model_tokens,
            concurrency,
        })
    }

    /// Requested CPU quota in millicores.
    #[must_use]
    pub const fn cpu_millis(&self) -> Option<u64> {
        self.cpu_millis
    }

    /// Requested memory ceiling in bytes.
    #[must_use]
    pub const fn memory_bytes(&self) -> Option<u64> {
        self.memory_bytes
    }

    /// Requested wall-clock limit in milliseconds.
    #[must_use]
    pub const fn wall_clock_ms(&self) -> Option<u64> {
        self.wall_clock_ms
    }

    /// Requested GPU device count, including explicit zero.
    #[must_use]
    pub const fn gpu_devices(&self) -> Option<u32> {
        self.gpu_devices
    }

    /// Requested model-token ceiling.
    #[must_use]
    pub const fn model_tokens(&self) -> Option<u64> {
        self.model_tokens
    }

    /// Requested maximum concurrent operations.
    #[must_use]
    pub const fn concurrency(&self) -> Option<u32> {
        self.concurrency
    }

    fn fingerprint_into(&self, mut fingerprint: Fingerprint) -> Fingerprint {
        fingerprint = fingerprint_optional_u64(fingerprint, self.cpu_millis);
        fingerprint = fingerprint_optional_u64(fingerprint, self.memory_bytes);
        fingerprint = fingerprint_optional_u64(fingerprint, self.wall_clock_ms);
        fingerprint = fingerprint_optional_u64(
            fingerprint,
            self.gpu_devices.map(u64::from),
        );
        fingerprint = fingerprint_optional_u64(fingerprint, self.model_tokens);
        fingerprint_optional_u64(fingerprint, self.concurrency.map(u64::from))
    }
}

/// Validated resource envelope bound to caller-owned opaque identities.
///
/// The task and workspace references are included in the fingerprint, so a plan
/// can be bound to the exact values that were validated without ElasticXxx
/// interpreting or owning those identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskResourceEnvelopeV1 {
    task_ref: String,
    workspace_ref: String,
    budget: TaskResourceBudgetV1,
    fingerprint: Fingerprint,
}

impl TaskResourceEnvelopeV1 {
    /// Construct one validated task resource envelope.
    ///
    /// # Errors
    ///
    /// Rejects blank, padded, overlong, control-bearing or secret-like
    /// references and invalid resource bounds.
    pub fn new(
        task_ref: impl Into<String>,
        workspace_ref: impl Into<String>,
        budget: TaskResourceBudgetV1,
    ) -> Result<Self, TaskResourceEnvelopeError> {
        let task_ref = validate_reference("task_ref", task_ref.into())?;
        let workspace_ref = validate_reference("workspace_ref", workspace_ref.into())?;
        let mut fingerprint = Fingerprint::EMPTY
            .text(TASK_RESOURCE_ENVELOPE_V1)
            .text(&task_ref)
            .text(&workspace_ref);
        fingerprint = budget.fingerprint_into(fingerprint);
        Ok(Self {
            task_ref,
            workspace_ref,
            budget,
            fingerprint,
        })
    }

    /// Opaque task identity reference, unchanged from the caller input.
    #[must_use]
    pub fn task_ref(&self) -> &str {
        &self.task_ref
    }

    /// Opaque exact-workspace reference, unchanged from the caller input.
    #[must_use]
    pub fn workspace_ref(&self) -> &str {
        &self.workspace_ref
    }

    /// Validated optional resource bounds.
    #[must_use]
    pub const fn budget(&self) -> &TaskResourceBudgetV1 {
        &self.budget
    }

    /// Structural identity of the contract, opaque references and all bounds.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Validation failures for the AXE-1 task resource envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskResourceEnvelopeError {
    InvalidReference { field: &'static str },
    ReferenceTooLong { field: &'static str, maximum: usize },
    ZeroBound { dimension: &'static str },
}

impl fmt::Display for TaskResourceEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReference { field } => {
                write!(formatter, "{field} must be a bounded non-secret label")
            }
            Self::ReferenceTooLong { field, maximum } => {
                write!(formatter, "{field} exceeds {maximum} bytes")
            }
            Self::ZeroBound { dimension } => {
                write!(formatter, "resource bound {dimension} must be positive")
            }
        }
    }
}

impl std::error::Error for TaskResourceEnvelopeError {}

fn validate_reference(
    field: &'static str,
    value: String,
) -> Result<String, TaskResourceEnvelopeError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.chars().any(char::is_control)
        || contains_secret_marker(&value)
    {
        return Err(TaskResourceEnvelopeError::InvalidReference { field });
    }
    if value.len() > MAX_TASK_RESOURCE_REF_BYTES {
        return Err(TaskResourceEnvelopeError::ReferenceTooLong {
            field,
            maximum: MAX_TASK_RESOURCE_REF_BYTES,
        });
    }
    Ok(value)
}

fn contains_secret_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["token=", "api_key=", "apikey=", "secret=", "-----begin"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn fingerprint_optional_u64(mut fingerprint: Fingerprint, value: Option<u64>) -> Fingerprint {
    match value {
        Some(value) => {
            fingerprint = fingerprint.number(1);
            fingerprint.number(value)
        }
        None => fingerprint.number(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(
        cpu_millis: Option<u64>,
        memory_bytes: Option<u64>,
        gpu_devices: Option<u32>,
    ) -> TaskResourceBudgetV1 {
        TaskResourceBudgetV1::new(cpu_millis, memory_bytes, None, gpu_devices, None, None)
            .expect("valid budget")
    }

    #[test]
    fn envelope_preserves_opaque_bindings_and_cpu_only_gpu_request() {
        let envelope = TaskResourceEnvelopeV1::new(
            "hub-task:run-17",
            "sha256:4f6a",
            budget(Some(1500), Some(1 << 30), Some(0)),
        )
        .expect("envelope");
        assert_eq!(envelope.task_ref(), "hub-task:run-17");
        assert_eq!(envelope.workspace_ref(), "sha256:4f6a");
        assert_eq!(envelope.budget().gpu_devices(), Some(0));
    }

    #[test]
    fn empty_budget_is_valid_but_zero_limits_are_rejected() {
        assert_eq!(TaskResourceBudgetV1::default(), TaskResourceBudgetV1::new(None, None, None, None, None, None).unwrap());
        for (dimension, result) in [
            ("cpu_millis", TaskResourceBudgetV1::new(Some(0), None, None, None, None, None)),
            ("memory_bytes", TaskResourceBudgetV1::new(None, Some(0), None, None, None, None)),
            ("wall_clock_ms", TaskResourceBudgetV1::new(None, None, Some(0), None, None, None)),
            ("model_tokens", TaskResourceBudgetV1::new(None, None, None, None, Some(0), None)),
            ("concurrency", TaskResourceBudgetV1::new(None, None, None, None, None, Some(0))),
        ] {
            assert_eq!(
                result,
                Err(TaskResourceEnvelopeError::ZeroBound { dimension })
            );
        }
    }

    #[test]
    fn opaque_references_reject_padding_controls_and_secret_markers() {
        let budget = TaskResourceBudgetV1::default();
        for invalid in ["", "  ", " task ", "task\nref", "secret://token=abc"] {
            assert!(matches!(
                TaskResourceEnvelopeV1::new(invalid, "workspace:1", budget),
                Err(TaskResourceEnvelopeError::InvalidReference { field: "task_ref" })
            ));
        }
        assert!(matches!(
            TaskResourceEnvelopeV1::new("task:1", "workspace:1 ", budget),
            Err(TaskResourceEnvelopeError::InvalidReference {
                field: "workspace_ref"
            })
        ));
    }

    #[test]
    fn oversized_reference_is_rejected() {
        let reference = "x".repeat(MAX_TASK_RESOURCE_REF_BYTES + 1);
        assert_eq!(
            TaskResourceEnvelopeV1::new(reference, "workspace:1", TaskResourceBudgetV1::default()),
            Err(TaskResourceEnvelopeError::ReferenceTooLong {
                field: "task_ref",
                maximum: MAX_TASK_RESOURCE_REF_BYTES
            })
        );
    }

    #[test]
    fn fingerprint_binds_task_workspace_and_every_budget_dimension() {
        let base = TaskResourceEnvelopeV1::new(
            "task:1",
            "workspace:1",
            budget(Some(1000), Some(1 << 30), Some(0)),
        )
        .unwrap();
        assert_ne!(
            base.fingerprint(),
            TaskResourceEnvelopeV1::new(
                "task:2",
                "workspace:1",
                budget(Some(1000), Some(1 << 30), Some(0)),
            )
            .unwrap()
            .fingerprint()
        );
        assert_ne!(
            base.fingerprint(),
            TaskResourceEnvelopeV1::new(
                "task:1",
                "workspace:2",
                budget(Some(1000), Some(1 << 30), Some(0)),
            )
            .unwrap()
            .fingerprint()
        );
        assert_ne!(
            base.fingerprint(),
            TaskResourceEnvelopeV1::new(
                "task:1",
                "workspace:1",
                budget(Some(2000), Some(1 << 30), Some(0)),
            )
            .unwrap()
            .fingerprint()
        );
    }
}
