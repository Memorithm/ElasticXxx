//! Adapter contract tests: real actions, enforced invariants.

use elastic_adapters::{AdapterError, ConcurrencyPermits, RamBudget};
use elastic_core::resource::{DimensionId, ObservationSignalId};
use elastic_core::TransitionMechanism;

#[test]
fn ram_budget_rejects_degenerate_configuration() {
    let result = RamBudget::new("b", 1024, 0, 512, 64, None);
    assert!(matches!(
        result,
        Err(AdapterError::InvalidBounds { min: 0, max: 512 })
    ));

    let result = RamBudget::new("b", 1024, 2048, 4096, 64, None);
    assert!(matches!(
        result,
        Err(AdapterError::InvalidBounds {
            min: 2048,
            max: 4096
        })
    ));

    let result = RamBudget::new("b", 1024, 64, 512, 600, None);
    assert!(matches!(
        result,
        Err(AdapterError::InitialOutOfBounds {
            initial: 600,
            min: 64,
            max: 512
        })
    ));
}

#[test]
fn ram_budget_resizes_for_real_within_bounds_and_steps() {
    let mut budget = RamBudget::new("cache", 1 << 20, 256, 1 << 18, 1024, Some(4096)).unwrap();

    // Real allocation: committed bytes are backed by an actual buffer.
    assert_eq!(budget.committed(), 1024);

    // In-bounds grow within the step limit.
    assert_eq!(budget.apply(4096), Ok((1024, 4096)));
    assert_eq!(budget.committed(), 4096);

    // Step limit is enforced against the current commitment.
    assert_eq!(
        budget.apply(32 * 1024),
        Err(AdapterError::StepLimitExceeded {
            from: 4096,
            to: 32 * 1024,
            max_step: 4096
        })
    );

    // Bounds are enforced even for small steps.
    assert_eq!(
        budget.apply(128),
        Err(AdapterError::TargetOutOfBounds {
            target: 128,
            min: 256,
            max: 1 << 18
        })
    );

    // Shrink within limits works (real release).
    assert_eq!(budget.apply(2048), Ok((4096, 2048)));
    assert_eq!(budget.committed(), 2048);
}

#[test]
fn preserve_contents_blocks_destroying_in_use_bytes() {
    let mut budget = RamBudget::new("store", 1 << 20, 128, 1 << 18, 4096, None).unwrap();
    budget.record_use(3000).unwrap();
    assert_eq!(budget.in_use(), 3000);

    // Shrinking below protected usage violates PreserveContents and is
    // refused by the adapter itself — planners cannot override this.
    assert_eq!(
        budget.apply(1024),
        Err(AdapterError::WouldViolateContents {
            target: 1024,
            in_use: 3000
        })
    );

    // Releasing usage re-enables shrinking.
    budget.release_use(2000).unwrap();
    assert_eq!(budget.apply(1024), Ok((4096, 1024)));

    // Over-release is caught.
    assert_eq!(
        budget.release_use(999_999),
        Err(AdapterError::WouldViolateContents {
            target: 0,
            in_use: 1000
        })
    );
    assert_eq!(
        budget.record_use(u64::MAX),
        Err(AdapterError::UsageOverflow {
            requested_total: u64::MAX,
            committed: 1024
        })
    );
}

#[test]
fn observations_are_derived_from_state_and_configuration() {
    let budget = RamBudget::new("cache", 10_000, 100, 5_000, 2_500, None).unwrap();
    let context = budget.observe();
    assert_eq!(context.get(ObservationSignalId::UTILIZATION), Some(0.25));
    assert_eq!(
        context.get(ObservationSignalId::FREE_CAPACITY),
        Some(7500.0)
    );

    // Deterministic across repeated observation.
    assert_eq!(context, budget.observe());
}

#[test]
fn candidates_are_grounded_and_carry_magnitudes() {
    let budget = RamBudget::new("cache", 1 << 20, 256, 1 << 18, 1024, None).unwrap();
    let candidate = budget.candidate(8192).unwrap();
    assert_eq!(candidate.mechanism(), TransitionMechanism::Reinterpret);
    assert_eq!(candidate.dimension(), &DimensionId::CAPACITY);
    assert_eq!(candidate.magnitude(), Some(8192));
    assert!(candidate.capability_grounded());
    assert!(candidate.is_declared_in(budget.ir()));

    // The EIR node lowers from a valid declaration with exactly one grounded
    // admission.
    assert_eq!(budget.ir().transitions().len(), 1);
}

#[test]
fn permits_refuse_stranding_and_overflow() {
    let mut pool = ConcurrencyPermits::new("workers", 16, 4).unwrap();
    assert_eq!(pool.width(), 4);

    for _ in 0..4 {
        pool.acquire().unwrap();
    }
    assert_eq!(
        pool.acquire(),
        Err(AdapterError::PermitOverflow {
            active: 5,
            width: 4
        })
    );

    // Shrinking below active holders would strand them.
    assert_eq!(
        pool.apply(2),
        Err(AdapterError::WouldStrandHolders {
            requested_width: 2,
            active: 4
        })
    );

    // Growing is fine while holders stay active.
    assert_eq!(pool.apply(8), Ok((4, 8)));
    pool.acquire().unwrap();

    pool.release().unwrap();
    pool.release().unwrap();
    pool.release().unwrap();
    pool.release().unwrap();
    pool.release().unwrap();
    assert_eq!(pool.active(), 0);
    assert_eq!(
        pool.release(),
        Err(AdapterError::PermitOverflow {
            active: 0,
            width: 8
        })
    );

    // Zero width is never valid.
    assert_eq!(
        pool.apply(0),
        Err(AdapterError::TargetOutOfBounds {
            target: 0,
            min: 1,
            max: 16
        })
    );
}

#[test]
fn adapters_and_their_ir_are_send_sync_plain_data() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RamBudget>();
    assert_send_sync::<ConcurrencyPermits>();
    assert_send_sync::<AdapterError>();
}
