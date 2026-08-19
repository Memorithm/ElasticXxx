# NOMAD: Non-Exclusive Memory Tiering via Transactional Page Migration

**Paper:** Lingfeng Xiang et al. *NOMAD: Non-Exclusive Memory Tiering via Transactional Page Migration*. OSDI 2024.

**Primary source:** USENIX OSDI 2024 paper: https://www.usenix.org/system/files/osdi24-xiang.pdf

## 1. Problem

**SOURCE-DERIVED.** Traditional memory tiering is exclusive: a page is present in one tier at a time. Under fast-tier pressure this can cause hot/cold swapping and expensive migration. Existing Linux page migration also commonly unmaps, copies, then remaps a page, placing migration on the critical path of user accesses.

## 2. Core mechanisms

**SOURCE-DERIVED.** NOMAD combines:

- **non-exclusive tiering**: recently promoted pages may retain a shadow copy in the slower tier;
- **transactional page migration (TPM)**: copy first while the source remains accessible, then atomically decide whether the migration can commit;
- **asynchronous promotion**: migration is moved off the critical path of normal user accesses;
- **shadow-aware demotion**: if the fast-tier master remains clean and consistent with its shadow, demotion can become a remapping operation instead of another copy.

## 3. Transaction semantics

**SOURCE-DERIVED.** TPM does not unmap before copying. It clears/tracks the dirty state, copies the page, then checks whether the page changed during the copy. If the page was dirtied, the transaction is aborted, the destination copy is discarded, and migration can be retried later. If it remained clean, the page-table mapping is switched to the fast-tier copy and the old page becomes a shadow copy.

This is a true example of a resource transition whose outcome is conditional on state observed *during* the transition.

## 4. Resource model

**SOURCE-DERIVED.** NOMAD assumes adjacent fast/slow memory tiers and manages page placement at OS page granularity. It does not itself decide page temperature; it relies on existing OS tracking/policy for which pages should migrate.

**INFERENCE.** NOMAD separates the **migration mechanism** from the **placement decision policy**. This separation is highly relevant to ElasticXxx.

## 5. Safety and invariants

**SOURCE-DERIVED.** NOMAD tracks master/shadow consistency and prevents shadowing from causing OOM by reclaiming shadow pages before ordinary pages under capacity-tier pressure. It disables TPM for multi-mapped pages when the cost/complexity of coordinated TLB shootdowns undermines the mechanism and falls back to standard synchronous migration.

## 6. Results

**SOURCE-DERIVED.** The paper reports up to 6× improvement over Linux TPP under memory pressure and up to 130% improvement over Memtis in cases described by the paper. These are workload/platform-specific results, not universal guarantees.

## 7. Elastic relation

| NOMAD mechanism | ElasticXxx disposition |
|---|---|
| Separate migration mechanism from placement policy | **ADOPT / GENERALIZE** |
| Copy while source remains usable | **ADOPT principle where semantics permit** |
| Commit/abort based on concurrent modification | **ADOPT / GENERALIZE** |
| Retry later after abort | **ADOPT as optional transition semantics** |
| Shadow copy retained after promotion | **ADAPT into REDUNDANCY / RESIDENCY dimensions** |
| Clean-shadow demotion by remap | **ADOPT conceptually** |
| Page granularity | **ADAPT** |
| Two adjacent memory tiers | **REJECT as general Elastic assumption** |
| OS hotness policy as external decision source | **ADAPT** |

## 8. Strong Elastic lesson

A transition should not be modeled merely as:

```text
StateA -> StateB
```

NOMAD demonstrates a richer pattern:

```text
PREPARE
  -> COPY while source remains valid
  -> VALIDATE concurrent state
  -> COMMIT new residency
      or
     ABORT destination copy
  -> optionally RETAIN old representation as shadow
```

**ELASTIC PROPOSAL.** `ElasticTransition` should be able to express speculative/transactional transitions with preconditions, concurrent-access semantics, commit validation, abort, retry, and optional retained replicas.

## 9. Identity versus residency

**ELASTIC INFERENCE.** NOMAD reinforces the distinction between a page's logical identity and its physical residency. During TPM there can temporarily be two physical copies associated with one logical page, yet only one mapping is authoritative at commit. This is a concrete prior-art example supporting ElasticXxx's separation of `IDENTITY`, `STATE`, and `RESIDENCY`.

## 10. Experiment suggested

Compare for an Elastic memory prototype:

1. stop-the-world copy/migrate;
2. copy-then-validate transactional migration;
3. copy + retained shadow replica;
4. no migration.

Measure useful progress, blocked time, migration success/abort rate, bandwidth consumed, memory overhead, retry count, and semantic correctness under concurrent writes.
