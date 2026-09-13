# Boolean guard EIR boundary

The Boolean guard envelope is additive to the legacy EIR v1 resource shape.

`ResourceSpec` continues to lower through the existing validated EIR path. A `GuardedResourceSpec` adds canonical `BooleanGuard` declarations that are lowered separately into `EirGuard` entries and combined with the normalized base resource as `EirGuardedResource`.

This boundary is deliberate:

- legacy unguarded EIR fingerprints and schema remain unchanged;
- every guard is already canonicalized and bound to stable predicate identities;
- dimension-scoped guards can target only declared elastic dimensions;
- transition-scoped guards can target only already-admitted transitions;
- guards are declarative eligibility data and cannot execute or authorize actuation;
- the combined guarded-resource fingerprint binds base EIR identity plus ordered guard identities;
- runtime interpretation of `True`, `False`, and `Unknown` is introduced only in later Boolean elasticity phases.

The Boolean guard envelope has its own schema constant, `EIR_BOOLEAN_GUARD_SCHEMA_VERSION`. Any later wire format must preserve these semantics and validation boundaries rather than introducing a second policy implementation.
