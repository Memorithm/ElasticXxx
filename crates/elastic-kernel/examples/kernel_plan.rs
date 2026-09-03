//! Minimal executable kernel-plan example for the EX5 public programming surface.
//!
//! Run with:
//! `cargo run -p elastic-kernel --example kernel_plan`

#![forbid(unsafe_code)]

use elastic::{BuiltinObjective, ContractId, Fingerprint, LogicalResourceId, ObjectiveId};
use elastic_kernel::{
    plan, BindingLimits, CapabilitySnapshot, Evidence, EvidenceUnit, FeatureRequirement,
    FeatureSupport, KernelCandidate, KernelRequirements, ObjectiveEvidence, SelectionOutcome,
    SelectionPolicy, StaticQuantity, SubgroupSupport, WorkgroupLimits,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resource = LogicalResourceId::new("attention#example")?;
    let contract = ContractId::new("attention-forward-v1")?;
    let latency = ObjectiveId::builtin(BuiltinObjective::Latency);

    let capabilities = CapabilitySnapshot::new(CapabilitySnapshot {
        workgroup_limits: WorkgroupLimits {
            max_invocations_per_axis: [256, 256, 64],
            max_invocations_per_workgroup: 256,
            max_workgroups_per_axis: 65_535,
            max_workgroup_storage_bytes: 32_768,
        },
        binding_limits: BindingLimits {
            max_bind_groups: 8,
            max_storage_buffer_binding_bytes: 128 << 20,
        },
        subgroup_support: SubgroupSupport::unsupported(),
        shader_f16: FeatureSupport::Known(false),
        matrix_ops: FeatureSupport::Unknown,
    })?;

    let requirements = KernelRequirements {
        invocations_per_workgroup: 64,
        invocations_per_axis: [64, 1, 1],
        workgroup_storage_bytes: 24_576,
        bind_groups: 2,
        max_storage_buffer_binding_bytes: 4096,
        subgroup_min_width: None,
        shader_f16: FeatureRequirement::NotRequired,
        matrix_ops: FeatureRequirement::NotRequired,
    };
    requirements.validate()?;

    let candidate = KernelCandidate::new(
        resource.clone(),
        elastic_kernel::RealizationIdentity::new("portable-q4")?,
        1,
        requirements,
        contract.clone(),
        ObjectiveEvidence::new().with(
            latency.clone(),
            Evidence::StaticEstimate(StaticQuantity {
                magnitude: 100,
                unit: EvidenceUnit::Nanoseconds,
            }),
        ),
    )?;

    let policy = SelectionPolicy::new(vec![latency], contract, true)?;
    let workload = Fingerprint::EMPTY.text("example/attention/b=1/h=8/n=128/d=64");

    match plan(&resource, workload, &capabilities, &policy, &[candidate]) {
        SelectionOutcome::Selected(record) => {
            println!(
                "resource={} realization={} capability_fingerprint={}",
                record.logical_resource_id().as_str(),
                record.selected_realization().as_str(),
                record.capability_fingerprint()
            );
            Ok(())
        }
        outcome => Err(format!("kernel plan was not selected: {outcome:?}").into()),
    }
}
