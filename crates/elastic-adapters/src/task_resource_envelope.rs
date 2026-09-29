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
///
/// References use a conservative `<namespace>:<identifier>` ASCII form so
/// free-form credential strings are not accepted as diagnostic labels.
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
    max_energy_microjoules: Option<u64>,
    minimum_thermal_margin_millicelsius: Option<u64>,
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
            max_energy_microjoules: None,
            minimum_thermal_margin_millicelsius: None,
        })
    }

    /// Set an optional maximum accumulated energy allowance.
    ///
    /// The value is a caller-owned task budget in microjoules. It is not a
    /// measured or predicted host-energy observation.
    pub fn with_max_energy_microjoules(
        mut self,
        max_energy_microjoules: Option<u64>,
    ) -> Result<Self, TaskResourceEnvelopeError> {
        if max_energy_microjoules == Some(0) {
            return Err(TaskResourceEnvelopeError::ZeroBound {
                dimension: "max_energy_microjoules",
            });
        }
        self.max_energy_microjoules = max_energy_microjoules;
        Ok(self)
    }

    /// Set the minimum caller-selected thermal margin in millidegrees Celsius.
    ///
    /// Zero is valid. This is a task requirement, not a claim about host
    /// temperature, cooling capacity, or the accuracy of a forecast.
    #[must_use]
    pub const fn with_minimum_thermal_margin_millicelsius(
        mut self,
        minimum_thermal_margin_millicelsius: Option<u64>,
    ) -> Self {
        self.minimum_thermal_margin_millicelsius = minimum_thermal_margin_millicelsius;
        self
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

    /// Maximum accumulated energy allowance in microjoules.
    #[must_use]
    pub const fn max_energy_microjoules(&self) -> Option<u64> {
        self.max_energy_microjoules
    }

    /// Minimum task-selected thermal margin in millidegrees Celsius.
    #[must_use]
    pub const fn minimum_thermal_margin_millicelsius(&self) -> Option<u64> {
        self.minimum_thermal_margin_millicelsius
    }

    fn fingerprint_into(&self, mut fingerprint: Fingerprint) -> Fingerprint {
        fingerprint = fingerprint_optional_u64(fingerprint, self.cpu_millis);
        fingerprint = fingerprint_optional_u64(fingerprint, self.memory_bytes);
        fingerprint = fingerprint_optional_u64(fingerprint, self.wall_clock_ms);
        fingerprint = fingerprint_optional_u64(fingerprint, self.gpu_devices.map(u64::from));
        fingerprint = fingerprint_optional_u64(fingerprint, self.model_tokens);
        fingerprint = fingerprint_optional_u64(fingerprint, self.concurrency.map(u64::from));
        if self.max_energy_microjoules.is_none()
            && self.minimum_thermal_margin_millicelsius.is_none()
        {
            return fingerprint;
        }
        fingerprint = fingerprint.text("elastic.task-resource-envelope-axe3-dimensions@1");
        fingerprint = fingerprint_optional_u64(fingerprint, self.max_energy_microjoules);
        fingerprint_optional_u64(fingerprint, self.minimum_thermal_margin_millicelsius)
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

    /// Validate a bounded CPU, memory, wall-clock and concurrency plan.
    ///
    /// The legacy plan surface fails closed when an envelope requests a
    /// dimension introduced after AXE-2. Use the advanced estimate method to
    /// validate GPU, token, energy, and thermal-margin estimates.
    pub fn preflight_plan(
        &self,
        cpu_millis: Option<u64>,
        memory_bytes: Option<u64>,
        wall_clock_ms: Option<u64>,
        concurrency: Option<u32>,
    ) -> Result<TaskResourcePlanV1, TaskResourcePlanError> {
        for (dimension, requested) in [
            ("gpu_devices", self.budget.gpu_devices.is_some()),
            ("model_tokens", self.budget.model_tokens.is_some()),
            (
                "max_energy_microjoules",
                self.budget.max_energy_microjoules.is_some(),
            ),
            (
                "minimum_thermal_margin_millicelsius",
                self.budget.minimum_thermal_margin_millicelsius.is_some(),
            ),
        ] {
            if requested {
                return Err(TaskResourcePlanError::UnsupportedDimension { dimension });
            }
        }

        self.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
            cpu_millis,
            memory_bytes,
            wall_clock_ms,
            concurrency,
            ..TaskResourcePlanEstimateV1::default()
        })
    }

    /// Validate all currently supported task-resource estimates.
    ///
    /// The values are caller-provided preflight estimates. This method does not
    /// observe host capacity, verify forecast provenance, or authorize
    /// enforcement or physical actuation. Energy is accumulated work in
    /// microjoules; thermal margin is a non-negative derived margin in
    /// millidegrees Celsius and must meet the task-selected minimum.
    pub fn preflight_plan_with_estimates(
        &self,
        estimates: TaskResourcePlanEstimateV1,
    ) -> Result<TaskResourcePlanV1, TaskResourcePlanError> {
        validate_planned_dimension("cpu_millis", self.budget.cpu_millis, estimates.cpu_millis)?;
        validate_planned_dimension(
            "memory_bytes",
            self.budget.memory_bytes,
            estimates.memory_bytes,
        )?;
        validate_planned_dimension(
            "wall_clock_ms",
            self.budget.wall_clock_ms,
            estimates.wall_clock_ms,
        )?;
        validate_planned_dimension(
            "concurrency",
            self.budget.concurrency.map(u64::from),
            estimates.concurrency.map(u64::from),
        )?;
        validate_planned_dimension_allow_zero(
            "gpu_devices",
            self.budget.gpu_devices.map(u64::from),
            estimates.gpu_devices.map(u64::from),
        )?;
        validate_planned_dimension(
            "model_tokens",
            self.budget.model_tokens,
            estimates.model_tokens,
        )?;
        validate_planned_dimension(
            "max_energy_microjoules",
            self.budget.max_energy_microjoules,
            estimates.energy_microjoules,
        )?;
        validate_minimum_dimension(
            "thermal_margin_millicelsius",
            self.budget.minimum_thermal_margin_millicelsius,
            estimates.thermal_margin_millicelsius,
        )?;

        Ok(TaskResourcePlanV1 {
            envelope_fingerprint: self.fingerprint,
            cpu_millis: estimates.cpu_millis,
            memory_bytes: estimates.memory_bytes,
            wall_clock_ms: estimates.wall_clock_ms,
            concurrency: estimates.concurrency,
            gpu_devices: estimates.gpu_devices,
            model_tokens: estimates.model_tokens,
            energy_microjoules: estimates.energy_microjoules,
            thermal_margin_millicelsius: estimates.thermal_margin_millicelsius,
        })
    }

    /// Structural identity of the contract, opaque references and all bounds.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// Preflight reservation values bound to the exact validated task envelope.
///
/// These values are requested plan inputs, not observations or evidence that a
/// host can enforce them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskResourcePlanV1 {
    envelope_fingerprint: Fingerprint,
    cpu_millis: Option<u64>,
    memory_bytes: Option<u64>,
    wall_clock_ms: Option<u64>,
    concurrency: Option<u32>,
    gpu_devices: Option<u32>,
    model_tokens: Option<u64>,
    energy_microjoules: Option<u64>,
    thermal_margin_millicelsius: Option<u64>,
}

/// Caller-supplied task resource estimates used only for bounded preflight.
///
/// No field is a host observation, a verified forecast, or proof of backend
/// enforcement. GPU count zero is allowed for an explicit CPU-only plan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskResourcePlanEstimateV1 {
    /// Requested CPU quota in millicores.
    pub cpu_millis: Option<u64>,
    /// Requested memory ceiling in bytes.
    pub memory_bytes: Option<u64>,
    /// Requested wall-clock limit in milliseconds.
    pub wall_clock_ms: Option<u64>,
    /// Requested maximum concurrent operations.
    pub concurrency: Option<u32>,
    /// GPU device count; zero is a valid explicit CPU-only plan.
    pub gpu_devices: Option<u32>,
    /// Planned model-token ceiling.
    pub model_tokens: Option<u64>,
    /// Planned accumulated energy in microjoules.
    pub energy_microjoules: Option<u64>,
    /// Estimated thermal margin in millidegrees Celsius.
    pub thermal_margin_millicelsius: Option<u64>,
}

impl TaskResourcePlanV1 {
    #[must_use]
    pub fn is_bound_to(&self, envelope: &TaskResourceEnvelopeV1) -> bool {
        self.envelope_fingerprint == envelope.fingerprint
    }

    #[must_use]
    pub const fn cpu_millis(&self) -> Option<u64> {
        self.cpu_millis
    }

    #[must_use]
    pub const fn memory_bytes(&self) -> Option<u64> {
        self.memory_bytes
    }

    #[must_use]
    pub const fn wall_clock_ms(&self) -> Option<u64> {
        self.wall_clock_ms
    }

    #[must_use]
    pub const fn concurrency(&self) -> Option<u32> {
        self.concurrency
    }

    #[must_use]
    pub const fn gpu_devices(&self) -> Option<u32> {
        self.gpu_devices
    }

    #[must_use]
    pub const fn model_tokens(&self) -> Option<u64> {
        self.model_tokens
    }

    #[must_use]
    pub const fn energy_microjoules(&self) -> Option<u64> {
        self.energy_microjoules
    }

    #[must_use]
    pub const fn thermal_margin_millicelsius(&self) -> Option<u64> {
        self.thermal_margin_millicelsius
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskResourcePlanError {
    UnsupportedDimension { dimension: &'static str },
    MissingEstimate { dimension: &'static str },
    ZeroEstimate { dimension: &'static str },
    ExceedsBound { dimension: &'static str },
    BelowMinimum { dimension: &'static str },
}

impl fmt::Display for TaskResourcePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedDimension { dimension } => {
                write!(
                    formatter,
                    "task resource dimension {dimension} is not supported yet"
                )
            }
            Self::MissingEstimate { dimension } => {
                write!(
                    formatter,
                    "task resource plan is missing requested dimension {dimension}"
                )
            }
            Self::ZeroEstimate { dimension } => {
                write!(
                    formatter,
                    "task resource plan dimension {dimension} must be positive"
                )
            }
            Self::ExceedsBound { dimension } => {
                write!(
                    formatter,
                    "task resource plan exceeds requested bound {dimension}"
                )
            }
            Self::BelowMinimum { dimension } => {
                write!(
                    formatter,
                    "task resource plan falls below requested minimum {dimension}"
                )
            }
        }
    }
}

impl std::error::Error for TaskResourcePlanError {}

fn validate_planned_dimension_allow_zero(
    dimension: &'static str,
    bound: Option<u64>,
    planned: Option<u64>,
) -> Result<(), TaskResourcePlanError> {
    if bound.is_some() && planned.is_none() {
        return Err(TaskResourcePlanError::MissingEstimate { dimension });
    }
    if let (Some(bound), Some(planned)) = (bound, planned) {
        if planned > bound {
            return Err(TaskResourcePlanError::ExceedsBound { dimension });
        }
    }
    Ok(())
}

fn validate_minimum_dimension(
    dimension: &'static str,
    minimum: Option<u64>,
    planned: Option<u64>,
) -> Result<(), TaskResourcePlanError> {
    if minimum.is_some() && planned.is_none() {
        return Err(TaskResourcePlanError::MissingEstimate { dimension });
    }
    if let (Some(minimum), Some(planned)) = (minimum, planned) {
        if planned < minimum {
            return Err(TaskResourcePlanError::BelowMinimum { dimension });
        }
    }
    Ok(())
}

fn validate_planned_dimension(
    dimension: &'static str,
    bound: Option<u64>,
    planned: Option<u64>,
) -> Result<(), TaskResourcePlanError> {
    if planned == Some(0) {
        return Err(TaskResourcePlanError::ZeroEstimate { dimension });
    }
    if bound.is_some() && planned.is_none() {
        return Err(TaskResourcePlanError::MissingEstimate { dimension });
    }
    if let (Some(bound), Some(planned)) = (bound, planned) {
        if planned > bound {
            return Err(TaskResourcePlanError::ExceedsBound { dimension });
        }
    }
    Ok(())
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
    if value.len() > MAX_TASK_RESOURCE_REF_BYTES {
        return Err(TaskResourceEnvelopeError::ReferenceTooLong {
            field,
            maximum: MAX_TASK_RESOURCE_REF_BYTES,
        });
    }
    let Some((namespace, identifier)) = value.split_once(':') else {
        return Err(TaskResourceEnvelopeError::InvalidReference { field });
    };
    let mut namespace_chars = namespace.chars();
    let namespace_is_valid = matches!(namespace_chars.next(), Some(first) if first.is_ascii_lowercase())
        && namespace_chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    let identifier_is_valid = !identifier.is_empty()
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'));
    if value.trim().is_empty()
        || value.trim() != value
        || value.chars().any(char::is_control)
        || !namespace_is_valid
        || !identifier_is_valid
        || contains_secret_marker(&value)
    {
        return Err(TaskResourceEnvelopeError::InvalidReference { field });
    }
    Ok(value)
}

fn contains_secret_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let namespace = lower.split(':').next().unwrap_or_default();
    let credential_namespaces = [
        "authorization",
        "bearer",
        "basic",
        "password",
        "passwd",
        "secret",
        "token",
        "api-key",
        "api_key",
        "apikey",
        "access-token",
        "private-key",
    ];
    let credential_markers = [
        "token=",
        "api_key=",
        "apikey=",
        "secret=",
        "-----begin",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "sk-",
        "akia",
        "bearer ",
        "basic ",
    ];
    credential_namespaces.contains(&namespace)
        || credential_markers
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
        assert_eq!(
            TaskResourceBudgetV1::default(),
            TaskResourceBudgetV1::new(None, None, None, None, None, None).unwrap()
        );
        for (dimension, result) in [
            (
                "cpu_millis",
                TaskResourceBudgetV1::new(Some(0), None, None, None, None, None),
            ),
            (
                "memory_bytes",
                TaskResourceBudgetV1::new(None, Some(0), None, None, None, None),
            ),
            (
                "wall_clock_ms",
                TaskResourceBudgetV1::new(None, None, Some(0), None, None, None),
            ),
            (
                "model_tokens",
                TaskResourceBudgetV1::new(None, None, None, None, Some(0), None),
            ),
            (
                "concurrency",
                TaskResourceBudgetV1::new(None, None, None, None, None, Some(0)),
            ),
        ] {
            assert_eq!(
                result,
                Err(TaskResourceEnvelopeError::ZeroBound { dimension })
            );
        }
    }

    #[test]
    fn opaque_references_reject_padding_controls_and_credential_forms() {
        let budget = TaskResourceBudgetV1::default();
        for invalid in [
            "",
            "  ",
            " task ",
            "task\nref",
            "secret://token=abc",
            "password=hunter2",
            "Authorization: Bearer hunter2",
            "authorization:bearer-hunter2",
            "token:opaque-value",
            "task:ghp_examplecredential",
            "task:sk-examplecredential",
        ] {
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
    fn preflight_plan_checks_four_bounds_and_binds_the_envelope() {
        let envelope = TaskResourceEnvelopeV1::new(
            "task:plan",
            "workspace:plan",
            TaskResourceBudgetV1::new(
                Some(2_000),
                Some(1 << 30),
                Some(60_000),
                None,
                None,
                Some(4),
            )
            .expect("valid budget"),
        )
        .expect("envelope");
        let plan = envelope
            .preflight_plan(Some(1_500), Some(1 << 29), Some(30_000), Some(2))
            .expect("plan within every bound");
        assert!(plan.is_bound_to(&envelope));
        assert_eq!(plan.cpu_millis(), Some(1_500));
        assert_eq!(plan.memory_bytes(), Some(1 << 29));
        assert_eq!(plan.wall_clock_ms(), Some(30_000));
        assert_eq!(plan.concurrency(), Some(2));

        assert_eq!(
            envelope.preflight_plan(Some(2_001), Some(1 << 29), Some(30_000), Some(2)),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "cpu_millis"
            })
        );
        assert_eq!(
            envelope.preflight_plan(Some(1_500), None, Some(30_000), Some(2)),
            Err(TaskResourcePlanError::MissingEstimate {
                dimension: "memory_bytes"
            })
        );
        assert_eq!(
            envelope.preflight_plan(Some(1_500), Some(1 << 29), Some(60_001), Some(2)),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "wall_clock_ms"
            })
        );
        assert_eq!(
            envelope.preflight_plan(Some(1_500), Some(1 << 29), Some(30_000), Some(5)),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "concurrency"
            })
        );
        assert_eq!(
            envelope.preflight_plan(Some(0), Some(1 << 29), Some(30_000), Some(2)),
            Err(TaskResourcePlanError::ZeroEstimate {
                dimension: "cpu_millis"
            })
        );
    }

    #[test]
    fn preflight_fails_closed_for_dimensions_outside_axe2() {
        let gpu_envelope = TaskResourceEnvelopeV1::new(
            "task:gpu",
            "workspace:gpu",
            TaskResourceBudgetV1::new(None, None, None, Some(0), None, None)
                .expect("CPU-only budget is valid"),
        )
        .expect("envelope");
        assert_eq!(
            gpu_envelope.preflight_plan(None, None, None, None),
            Err(TaskResourcePlanError::UnsupportedDimension {
                dimension: "gpu_devices"
            })
        );

        let token_envelope = TaskResourceEnvelopeV1::new(
            "task:tokens",
            "workspace:tokens",
            TaskResourceBudgetV1::new(None, None, None, None, Some(100), None)
                .expect("token budget is valid"),
        )
        .expect("envelope");
        assert_eq!(
            token_envelope.preflight_plan(None, None, None, None),
            Err(TaskResourcePlanError::UnsupportedDimension {
                dimension: "model_tokens"
            })
        );
    }

    #[test]
    fn axe3_estimates_validate_gpu_tokens_energy_and_thermal_margin() {
        let budget = TaskResourceBudgetV1::new(None, None, None, Some(2), Some(1_000), None)
            .expect("valid GPU and token bounds")
            .with_max_energy_microjoules(Some(4_000_000))
            .expect("positive energy bound")
            .with_minimum_thermal_margin_millicelsius(Some(750));
        let envelope =
            TaskResourceEnvelopeV1::new("task:axe3", "workspace:axe3", budget).expect("envelope");
        let estimates = TaskResourcePlanEstimateV1 {
            gpu_devices: Some(1),
            model_tokens: Some(800),
            energy_microjoules: Some(3_500_000),
            thermal_margin_millicelsius: Some(1_000),
            ..TaskResourcePlanEstimateV1::default()
        };
        let plan = envelope
            .preflight_plan_with_estimates(estimates)
            .expect("all AXE-3 estimates satisfy the task bounds");
        assert!(plan.is_bound_to(&envelope));
        assert_eq!(plan.gpu_devices(), Some(1));
        assert_eq!(plan.model_tokens(), Some(800));
        assert_eq!(plan.energy_microjoules(), Some(3_500_000));
        assert_eq!(plan.thermal_margin_millicelsius(), Some(1_000));

        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                gpu_devices: Some(3),
                ..estimates
            }),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "gpu_devices"
            })
        );
        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                model_tokens: None,
                ..estimates
            }),
            Err(TaskResourcePlanError::MissingEstimate {
                dimension: "model_tokens"
            })
        );
        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                energy_microjoules: Some(4_000_001),
                ..estimates
            }),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "max_energy_microjoules"
            })
        );
        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                thermal_margin_millicelsius: Some(749),
                ..estimates
            }),
            Err(TaskResourcePlanError::BelowMinimum {
                dimension: "thermal_margin_millicelsius"
            })
        );
        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                model_tokens: Some(0),
                ..estimates
            }),
            Err(TaskResourcePlanError::ZeroEstimate {
                dimension: "model_tokens"
            })
        );
    }

    #[test]
    fn axe3_zero_gpu_estimate_is_valid_only_for_cpu_only_envelope() {
        let envelope = TaskResourceEnvelopeV1::new(
            "task:cpu-only",
            "workspace:cpu-only",
            TaskResourceBudgetV1::new(None, None, None, Some(0), None, None)
                .expect("zero GPU is an explicit CPU-only bound"),
        )
        .expect("envelope");
        assert_eq!(
            envelope
                .preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                    gpu_devices: Some(0),
                    ..TaskResourcePlanEstimateV1::default()
                })
                .expect("CPU-only plan"),
            TaskResourcePlanV1 {
                envelope_fingerprint: envelope.fingerprint(),
                cpu_millis: None,
                memory_bytes: None,
                wall_clock_ms: None,
                concurrency: None,
                gpu_devices: Some(0),
                model_tokens: None,
                energy_microjoules: None,
                thermal_margin_millicelsius: None,
            }
        );
        assert_eq!(
            envelope.preflight_plan_with_estimates(TaskResourcePlanEstimateV1 {
                gpu_devices: Some(1),
                ..TaskResourcePlanEstimateV1::default()
            }),
            Err(TaskResourcePlanError::ExceedsBound {
                dimension: "gpu_devices"
            })
        );
    }

    #[test]
    fn advanced_budget_changes_the_envelope_fingerprint() {
        let base = TaskResourceEnvelopeV1::new(
            "task:fingerprint",
            "workspace:fingerprint",
            TaskResourceBudgetV1::default(),
        )
        .unwrap();
        let advanced_budget = TaskResourceBudgetV1::default()
            .with_max_energy_microjoules(Some(1))
            .unwrap();
        let advanced = TaskResourceEnvelopeV1::new(
            "task:fingerprint",
            "workspace:fingerprint",
            advanced_budget,
        )
        .unwrap();
        assert_ne!(base.fingerprint(), advanced.fingerprint());
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
