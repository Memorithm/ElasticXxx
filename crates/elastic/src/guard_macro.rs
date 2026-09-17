//! Declarative syntax over the existing public Boolean guard builders.

/// Build a fallible, stable-key Boolean guard without writing an expression tree.
///
/// The `when` expression must be enclosed in parentheses. Its atoms are paths
/// to registered [`crate::PredicateKey`] values, not Rust `bool` values or
/// telemetry expressions. Supported syntax is `true`, `false`, `!`, `^`, `&&`,
/// `||`, nested parentheses, and `implies((antecedent), (consequent))`.
/// Precedence is `!`, then `^`, then `&&`, then `||`. Repeated conjunctions and
/// disjunctions canonicalize through the core. Repeated XORs associate to the
/// right; parentheses can select a different structural grouping.
///
/// This macro returns `Result<BooleanGuard, ElasticGuardError>`. It builds the
/// ordinary core AST, resolves every key before canonicalization, and invokes
/// the same bounded [`crate::BooleanGuard::when`] constructor as the manual API.
/// It does not evaluate telemetry, short-circuit key validation, register keys,
/// or authorize actuation. Missing runtime facts remain `Unknown` when the
/// resulting guard is later evaluated. Registry and scope expressions are each
/// evaluated once. Keys and the registry are borrowed, not consumed.
///
/// Very large token expressions can reach Rust's compile-time macro recursion
/// limit; use the typed builder for generated policies instead of raising that
/// limit without review. Core expression depth/node limits still apply.
///
/// ```
/// use elastic::{elastic_guard, predicate, ElasticPredicates, GuardScope, TruthValue};
/// use std::collections::BTreeMap;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let capacity = predicate("application.ram", "capacity-ok")?;
/// let pressure = predicate("application.ram", "pressure-critical")?;
/// let predicates = ElasticPredicates::new([capacity.clone(), pressure.clone()])?;
/// let guard = elastic_guard! {
///     predicates: predicates,
///     scope: GuardScope::Resource,
///     when: (capacity && !pressure),
/// }?;
/// let facts = BTreeMap::from([(capacity, TruthValue::True)]);
/// assert_eq!(guard.evaluate(&facts)?, TruthValue::Unknown);
/// # Ok(())
/// # }
/// ```
///
/// Numerical comparisons belong in explicit predicate derivation, not here:
///
/// ```compile_fail
/// use elastic::{elastic_guard, ElasticPredicates, GuardScope};
/// let predicates = ElasticPredicates::empty();
/// let _ = elastic_guard! {
///     predicates: predicates, scope: GuardScope::Resource, when: (0.9 > 0.8)
/// };
/// ```
///
/// Function calls in a policy expression are not executed:
///
/// ```compile_fail
/// use elastic::{elastic_guard, predicate, ElasticPredicates, GuardScope, PredicateKey};
/// fn guessed() -> PredicateKey { predicate("unsafe.source", "guess").unwrap() }
/// let predicates = ElasticPredicates::empty();
/// let _ = elastic_guard! {
///     predicates: predicates, scope: GuardScope::Resource, when: (guessed())
/// };
/// ```
///
/// A Rust Boolean is not a stable predicate identity:
///
/// ```compile_fail
/// use elastic::{elastic_guard, ElasticPredicates, GuardScope};
/// let predicates = ElasticPredicates::empty();
/// let guessed = true;
/// let _ = elastic_guard! {
///     predicates: predicates, scope: GuardScope::Resource, when: (guessed)
/// };
/// ```
///
/// Incomplete expressions are rejected at compilation:
///
/// ```compile_fail
/// use elastic::{elastic_guard, ElasticPredicates, GuardScope};
/// let predicates = ElasticPredicates::empty();
/// let _ = elastic_guard! {
///     predicates: predicates, scope: GuardScope::Resource, when: (true &&)
/// };
/// ```
#[macro_export]
macro_rules! elastic_guard {
    (predicates: $predicates:expr, scope: $scope:expr, when: ($($expression:tt)+) $(,)?) => {{
        let __elastic_predicates = &$predicates;
        let __elastic_scope = $scope;
        $crate::elastic_guard!(@or __elastic_predicates [] $($expression)+)
            .and_then(|__elastic_expression| {
                $crate::BooleanGuard::when(
                    __elastic_scope,
                    __elastic_predicates.registry().clone(),
                    __elastic_expression,
                )
                .map_err($crate::ElasticGuardError::Canonicalization)
            })
    }};
    (@or $predicates:ident [$($left:tt)*] || $($right:tt)+) => {
        $crate::elastic_guard!(@and $predicates [] $($left)*)
            .and_then(|__left| {
                $crate::elastic_guard!(@or $predicates [] $($right)+)
                    .map(|__right| $crate::BoolExpr::any([__left, __right]))
            })
    };
    (@or $predicates:ident [$($left:tt)*]) => {
        $crate::elastic_guard!(@and $predicates [] $($left)*)
    };
    (@or $predicates:ident [$($left:tt)*] $next:tt $($rest:tt)*) => {
        $crate::elastic_guard!(@or $predicates [$($left)* $next] $($rest)*)
    };
    (@and $predicates:ident [$($left:tt)*] && $($right:tt)+) => {
        $crate::elastic_guard!(@xor $predicates [] $($left)*)
            .and_then(|__left| {
                $crate::elastic_guard!(@and $predicates [] $($right)+)
                    .map(|__right| $crate::BoolExpr::all([__left, __right]))
            })
    };
    (@and $predicates:ident [$($left:tt)*]) => {
        $crate::elastic_guard!(@xor $predicates [] $($left)*)
    };
    (@and $predicates:ident [$($left:tt)*] $next:tt $($rest:tt)*) => {
        $crate::elastic_guard!(@and $predicates [$($left)* $next] $($rest)*)
    };
    (@xor $predicates:ident [$($left:tt)*] ^ $($right:tt)+) => {
        $crate::elastic_guard!(@not $predicates $($left)*)
            .and_then(|__left| {
                $crate::elastic_guard!(@xor $predicates [] $($right)+)
                    .map(|__right| $crate::ElasticGuard::xor(__left, __right))
            })
    };
    (@xor $predicates:ident [$($left:tt)*]) => {
        $crate::elastic_guard!(@not $predicates $($left)*)
    };
    (@xor $predicates:ident [$($left:tt)*] $next:tt $($rest:tt)*) => {
        $crate::elastic_guard!(@xor $predicates [$($left)* $next] $($rest)*)
    };
    (@not $predicates:ident ! $($rest:tt)+) => {
        $crate::elastic_guard!(@not $predicates $($rest)+).map($crate::BoolExpr::negate)
    };
    (@not $predicates:ident ($($inner:tt)+)) => {
        $crate::elastic_guard!(@or $predicates [] $($inner)+)
    };
    (@not $predicates:ident implies(($($left:tt)+), ($($right:tt)+))) => {
        $crate::elastic_guard!(@or $predicates [] $($left)+)
            .and_then(|__left| {
                $crate::elastic_guard!(@or $predicates [] $($right)+)
                    .map(|__right| $crate::ElasticGuard::implies(__left, __right))
            })
    };
    (@not $predicates:ident true) => {
        ::core::result::Result::<$crate::BoolExpr, $crate::ElasticGuardError>::Ok(
            $crate::BoolExpr::Const(true),
        )
    };
    (@not $predicates:ident false) => {
        ::core::result::Result::<$crate::BoolExpr, $crate::ElasticGuardError>::Ok(
            $crate::BoolExpr::Const(false),
        )
    };
    (@not $predicates:ident $key:path) => {
        $predicates.atom(&$key)
    };
    ($($unsupported:tt)*) => {
        ::core::compile_error!(
            "expected elastic_guard! { predicates: registry, scope: scope, when: (Boolean expression) }; use stable key paths, !, ^, &&, ||, parentheses, or implies((a), (b))"
        )
    };
}
