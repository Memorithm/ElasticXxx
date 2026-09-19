//! Procedural macros for ElasticXxx.
//!
//! [`ElasticResource`] and the function-like [`elastic!`](elastic) language
//! surface lower declarations into exactly the same typed semantic structures a
//! programmer could build by hand with `elastic_core::resource::ResourceSpec`.
//! The macros contain **no** independent runtime semantics: every resource body
//! maps onto the same derive/builder path and validation remains in the typed
//! core (`ResourceSpecBuilder::build`).
//!
//! ```ignore
//! #[derive(ElasticResource)]
//! #[elastic(
//!     class(representational),
//!     id("session-kv"),
//!     allow(representation, residency),
//!     preserve(contents),
//!     optimize(latency),
//!     admit(reencode @ representation),
//! )]
//! struct SessionKv;
//! ```
//!
//! Expansion: an inherent associated function
//! `resource_spec() -> Result<ResourceSpec, ResourceSpecError>` building the
//! declaration through the ordinary public API. Fallible fragments (custom
//! terms, contract identifiers) propagate structured errors through `?`;
//! generated code never panics and never calls `unwrap`.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    braced, parenthesized, parse_macro_input, Attribute, Data, DeriveInput, Ident, LitInt, LitStr,
    Token, Visibility,
};

/// Declare an elastic resource.
///
/// See the crate documentation for the supported attribute grammar. The
/// attribute lowers to the ordinary `elastic-core` builder API; there is no
/// second semantic implementation.
#[proc_macro_derive(ElasticResource, attributes(elastic))]
pub fn derive_elastic_resource(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare Elastic resources with an embedded Rust DSL.
///
/// A standalone `resource` produces one named resource module. A `document`
/// groups multiple resource modules and lowers them through the existing
/// `EirDocumentBuilder`. ELANG3 `group` blocks add typed dependencies, shared
/// pseudo-Boolean budgets and explicitly owned cross-resource invariants through
/// the public core/EIR contracts. Resource bodies accept exactly the same
/// declaration fragments as `#[derive(ElasticResource)]`, so the DSL owns no
/// independent resource, planning, transaction, or runtime semantics.
///
/// ```ignore
/// elastic! {
///     pub resource session_kv {
///         class(representational);
///         id("session-kv");
///         allow(representation, residency);
///         preserve(contents);
///         optimize(latency);
///         admit(reencode @ representation);
///         capability(reencode @ representation);
///     }
/// }
///
/// let spec = session_kv::resource_spec()?;
/// ```
#[proc_macro]
pub fn elastic(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ElasticDslInput);
    expand_elastic_dsl(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

mod kw {
    syn::custom_keyword!(at_least);
    syn::custom_keyword!(at_most);
    syn::custom_keyword!(budget);
    syn::custom_keyword!(constraint);
    syn::custom_keyword!(contract);
    syn::custom_keyword!(depends);
    syn::custom_keyword!(dimension);
    syn::custom_keyword!(document);
    syn::custom_keyword!(equivalent);
    syn::custom_keyword!(exactly);
    syn::custom_keyword!(group);
    syn::custom_keyword!(guard);
    syn::custom_keyword!(hint);
    syn::custom_keyword!(id);
    syn::custom_keyword!(invariant);
    syn::custom_keyword!(maximum);
    syn::custom_keyword!(maximize);
    syn::custom_keyword!(members);
    syn::custom_keyword!(minimize);
    syn::custom_keyword!(objective);
    syn::custom_keyword!(owner);
    syn::custom_keyword!(participants);
    syn::custom_keyword!(policy);
    syn::custom_keyword!(predicate);
    syn::custom_keyword!(quantum);
    syn::custom_keyword!(requires);
    syn::custom_keyword!(resource);
    syn::custom_keyword!(scale);
    syn::custom_keyword!(target);
    syn::custom_keyword!(term);
    syn::custom_keyword!(transition);
    syn::custom_keyword!(unit);
    syn::custom_keyword!(version);
    syn::custom_keyword!(when);
}

struct ElasticResourceDsl {
    visibility: Visibility,
    name: Ident,
    body: proc_macro2::TokenStream,
}

struct ElasticDocumentResourceDsl {
    name: Ident,
    body: proc_macro2::TokenStream,
}

struct ElasticGroupDependencyDsl {
    dependent: Ident,
    required: Ident,
}

struct ElasticBudgetTermDsl {
    resource: Ident,
    predicate_namespace: String,
    predicate_name: String,
    weight: i128,
}

struct ElasticBudgetDsl {
    name: Ident,
    unit: String,
    quantum: u64,
    maximum: i128,
    terms: Vec<ElasticBudgetTermDsl>,
}

struct ElasticInvariantDsl {
    contract: String,
    owner: Ident,
    participants: Vec<Ident>,
}

struct ElasticGroupDsl {
    name: Ident,
    members: Vec<Ident>,
    dependencies: Vec<ElasticGroupDependencyDsl>,
    budgets: Vec<ElasticBudgetDsl>,
    invariants: Vec<ElasticInvariantDsl>,
}

enum ElasticPolicyGuardScopeDsl {
    Resource,
    Dimension(TermRef),
    Transition {
        mechanism: &'static str,
        dimension: TermRef,
    },
}

struct ElasticPolicyPredicateDsl {
    alias: Ident,
    namespace: String,
    name: String,
}

struct ElasticPolicyGuardDsl {
    scope: ElasticPolicyGuardScopeDsl,
    expression: proc_macro2::TokenStream,
}

enum ElasticPolicyConstraintDsl {
    AtMost {
        maximum: usize,
        predicates: Vec<Ident>,
    },
    AtLeast {
        minimum: usize,
        predicates: Vec<Ident>,
    },
    Exactly {
        exact: usize,
        predicates: Vec<Ident>,
    },
    Requires {
        feature: Ident,
        required: Ident,
    },
    Equivalent {
        left: Ident,
        right: Ident,
    },
    Budget {
        unit: String,
        quantum: u64,
        maximum: i128,
        terms: Vec<(Ident, i128)>,
    },
}

struct ElasticPolicyObjectiveDsl {
    objective: TermRef,
    direction: &'static str,
    unit: String,
    quantum: u64,
}

struct ElasticPolicyDsl {
    name: Ident,
    id: String,
    version: (u32, u32, u32),
    target: Ident,
    predicates: Vec<ElasticPolicyPredicateDsl>,
    guards: Vec<ElasticPolicyGuardDsl>,
    constraints: Vec<ElasticPolicyConstraintDsl>,
    objectives: Vec<ElasticPolicyObjectiveDsl>,
    hints: Vec<(String, String)>,
}

struct ElasticDocumentDsl {
    visibility: Visibility,
    name: Ident,
    resources: Vec<ElasticDocumentResourceDsl>,
    groups: Vec<ElasticGroupDsl>,
    policies: Vec<ElasticPolicyDsl>,
}

enum ElasticDslInput {
    Resource(ElasticResourceDsl),
    Document(ElasticDocumentDsl),
}

impl Parse for ElasticDslInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let visibility: Visibility = input.parse()?;
        if input.peek(kw::resource) {
            input.parse::<kw::resource>()?;
            let name: Ident = input.parse()?;
            let content;
            braced!(content in input);
            let body: proc_macro2::TokenStream = content.parse()?;
            if !input.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "elastic! single-resource form accepts exactly one resource; use `document NAME { ... }` for multi-resource declarations",
                ));
            }
            return Ok(Self::Resource(ElasticResourceDsl {
                visibility,
                name,
                body,
            }));
        }
        if input.peek(kw::document) {
            input.parse::<kw::document>()?;
            let name: Ident = input.parse()?;
            let content;
            braced!(content in input);
            let mut resources = Vec::new();
            let mut groups = Vec::new();
            let mut policies = Vec::new();
            let mut resource_names = std::collections::BTreeSet::new();
            let mut group_names = std::collections::BTreeSet::new();
            let mut policy_names = std::collections::BTreeSet::new();
            while !content.is_empty() {
                if content.peek(kw::resource) {
                    content.parse::<kw::resource>()?;
                    let resource_name: Ident = content.parse()?;
                    if matches!(
                        resource_name.to_string().as_str(),
                        "document" | "grouped_document"
                    ) {
                        return Err(syn::Error::new(
                            resource_name.span(),
                            "resource module name is reserved by the generated document API",
                        ));
                    }
                    if !resource_names.insert(resource_name.to_string()) {
                        return Err(syn::Error::new(
                            resource_name.span(),
                            format!(
                                "duplicate resource module `{resource_name}` in elastic! document"
                            ),
                        ));
                    }
                    let resource_content;
                    braced!(resource_content in content);
                    let body: proc_macro2::TokenStream = resource_content.parse()?;
                    resources.push(ElasticDocumentResourceDsl {
                        name: resource_name,
                        body,
                    });
                    continue;
                }
                if content.peek(kw::group) {
                    let group = parse_group_dsl(&content)?;
                    if resource_names.contains(&group.name.to_string())
                        || !group_names.insert(group.name.to_string())
                    {
                        return Err(syn::Error::new(
                            group.name.span(),
                            format!(
                                "duplicate or colliding resource group `{}` in elastic! document",
                                group.name
                            ),
                        ));
                    }
                    groups.push(group);
                    continue;
                }
                if content.peek(kw::policy) {
                    let policy = parse_policy_dsl(&content)?;
                    let policy_name = policy.name.to_string();
                    if matches!(policy_name.as_str(), "document" | "grouped_document")
                        || resource_names.contains(&policy_name)
                        || group_names.contains(&policy_name)
                        || !policy_names.insert(policy_name.clone())
                    {
                        return Err(syn::Error::new(
                            policy.name.span(),
                            format!(
                                "duplicate, reserved, or colliding policy module `{}` in elastic! document",
                                policy.name
                            ),
                        ));
                    }
                    policies.push(policy);
                    continue;
                }
                return Err(syn::Error::new(
                    content.span(),
                    "elastic! document bodies accept only `resource NAME { ... }`, `group NAME { ... }`, and `policy NAME { ... }` declarations",
                ));
            }
            if resources.is_empty() {
                return Err(syn::Error::new(
                    name.span(),
                    "elastic! document must contain at least one resource",
                ));
            }
            validate_group_references(&groups, &resource_names)?;
            validate_policy_references(&policies, &resource_names)?;
            if !input.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "unexpected tokens after elastic! document declaration",
                ));
            }
            return Ok(Self::Document(ElasticDocumentDsl {
                visibility,
                name,
                resources,
                groups,
                policies,
            }));
        }
        Err(syn::Error::new(
            input.span(),
            "expected `resource NAME { ... }` or `document NAME { ... }` after optional visibility",
        ))
    }
}

fn parse_policy_dsl(input: ParseStream<'_>) -> syn::Result<ElasticPolicyDsl> {
    input.parse::<kw::policy>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    let mut id = None;
    let mut version = None;
    let mut target = None;
    let mut predicates = Vec::new();
    let mut guards = Vec::new();
    let mut constraints = Vec::new();
    let mut objectives = Vec::new();
    let mut hints = Vec::new();
    let mut aliases = std::collections::BTreeSet::new();

    while !content.is_empty() {
        if content.peek(kw::id) {
            let keyword: kw::id = content.parse()?;
            if id.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "policy id(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let value: LitStr = inner.parse()?;
            expect_exhausted(&inner, "id")?;
            content.parse::<Token![;]>()?;
            id = Some(value.value());
            continue;
        }
        if content.peek(kw::version) {
            let keyword: kw::version = content.parse()?;
            if version.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "policy version(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let major: LitInt = inner.parse()?;
            inner.parse::<Token![,]>()?;
            let minor: LitInt = inner.parse()?;
            inner.parse::<Token![,]>()?;
            let patch: LitInt = inner.parse()?;
            expect_exhausted(&inner, "version")?;
            content.parse::<Token![;]>()?;
            version = Some((
                major.base10_parse::<u32>()?,
                minor.base10_parse::<u32>()?,
                patch.base10_parse::<u32>()?,
            ));
            continue;
        }
        if content.peek(kw::target) {
            let keyword: kw::target = content.parse()?;
            if target.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "policy target(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let resource: Ident = inner.parse()?;
            expect_exhausted(&inner, "target")?;
            content.parse::<Token![;]>()?;
            target = Some(resource);
            continue;
        }
        if content.peek(kw::predicate) {
            content.parse::<kw::predicate>()?;
            let inner;
            parenthesized!(inner in content);
            let alias: Ident = inner.parse()?;
            inner.parse::<Token![,]>()?;
            let namespace: LitStr = inner.parse()?;
            inner.parse::<Token![,]>()?;
            let predicate_name: LitStr = inner.parse()?;
            expect_exhausted(&inner, "predicate")?;
            content.parse::<Token![;]>()?;
            if !aliases.insert(alias.to_string()) {
                return Err(syn::Error::new(
                    alias.span(),
                    format!("duplicate policy predicate alias `{alias}`"),
                ));
            }
            predicates.push(ElasticPolicyPredicateDsl {
                alias,
                namespace: namespace.value(),
                name: predicate_name.value(),
            });
            continue;
        }
        if content.peek(kw::guard) {
            content.parse::<kw::guard>()?;
            let scope = if content.peek(kw::resource) {
                content.parse::<kw::resource>()?;
                ElasticPolicyGuardScopeDsl::Resource
            } else if content.peek(kw::dimension) {
                content.parse::<kw::dimension>()?;
                let inner;
                parenthesized!(inner in content);
                let dimension = parse_term(&inner, "dimension", DIMENSIONS)?;
                expect_exhausted(&inner, "dimension")?;
                ElasticPolicyGuardScopeDsl::Dimension(dimension)
            } else if content.peek(kw::transition) {
                content.parse::<kw::transition>()?;
                let inner;
                parenthesized!(inner in content);
                let mechanism = parse_mechanism(&inner)?;
                inner.parse::<Token![@]>()?;
                let dimension = parse_term(&inner, "dimension", DIMENSIONS)?;
                expect_exhausted(&inner, "transition")?;
                ElasticPolicyGuardScopeDsl::Transition {
                    mechanism,
                    dimension,
                }
            } else {
                return Err(syn::Error::new(
                    content.span(),
                    "policy guard expects resource, dimension(...), or transition(<mechanism> @ <dimension>) scope",
                ));
            };
            content.parse::<kw::when>()?;
            let expression;
            parenthesized!(expression in content);
            let expression: proc_macro2::TokenStream = expression.parse()?;
            if expression.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "policy guard when(...) expression must not be empty",
                ));
            }
            content.parse::<Token![;]>()?;
            guards.push(ElasticPolicyGuardDsl { scope, expression });
            continue;
        }
        if content.peek(kw::constraint) {
            content.parse::<kw::constraint>()?;
            constraints.push(parse_policy_constraint_dsl(&content)?);
            continue;
        }
        if content.peek(kw::objective) {
            content.parse::<kw::objective>()?;
            let objective = parse_term(&content, "objective", OBJECTIVES)?;
            let direction = if content.peek(kw::minimize) {
                content.parse::<kw::minimize>()?;
                "Minimize"
            } else if content.peek(kw::maximize) {
                content.parse::<kw::maximize>()?;
                "Maximize"
            } else {
                return Err(syn::Error::new(
                    content.span(),
                    "policy objective expects minimize or maximize",
                ));
            };
            content.parse::<kw::unit>()?;
            let unit_inner;
            parenthesized!(unit_inner in content);
            let unit: LitStr = unit_inner.parse()?;
            expect_exhausted(&unit_inner, "unit")?;
            content.parse::<kw::quantum>()?;
            let quantum_inner;
            parenthesized!(quantum_inner in content);
            let quantum: LitInt = quantum_inner.parse()?;
            expect_exhausted(&quantum_inner, "quantum")?;
            content.parse::<Token![;]>()?;
            objectives.push(ElasticPolicyObjectiveDsl {
                objective,
                direction,
                unit: unit.value(),
                quantum: quantum.base10_parse::<u64>()?,
            });
            continue;
        }
        if content.peek(kw::hint) {
            content.parse::<kw::hint>()?;
            let inner;
            parenthesized!(inner in content);
            let key: LitStr = inner.parse()?;
            inner.parse::<Token![,]>()?;
            let value: LitStr = inner.parse()?;
            expect_exhausted(&inner, "hint")?;
            content.parse::<Token![;]>()?;
            hints.push((key.value(), value.value()));
            continue;
        }
        return Err(syn::Error::new(
            content.span(),
            "unsupported policy declaration; expected id, version, target, predicate, guard, constraint, objective, or hint",
        ));
    }

    let span = name.span();
    let id = id.ok_or_else(|| syn::Error::new(span, "policy is missing mandatory id(\"...\")"))?;
    let version = version.ok_or_else(|| {
        syn::Error::new(
            span,
            "policy is missing mandatory version(major, minor, patch)",
        )
    })?;
    let target = target.ok_or_else(|| {
        syn::Error::new(span, "policy is missing mandatory target(resource_module)")
    })?;
    validate_policy_alias_references(&guards, &constraints, &aliases)?;
    Ok(ElasticPolicyDsl {
        name,
        id,
        version,
        target,
        predicates,
        guards,
        constraints,
        objectives,
        hints,
    })
}

fn parse_policy_constraint_dsl(input: ParseStream<'_>) -> syn::Result<ElasticPolicyConstraintDsl> {
    if input.peek(kw::at_most) || input.peek(kw::at_least) || input.peek(kw::exactly) {
        enum Kind {
            AtMost,
            AtLeast,
            Exactly,
        }
        let kind = if input.peek(kw::at_most) {
            input.parse::<kw::at_most>()?;
            Kind::AtMost
        } else if input.peek(kw::at_least) {
            input.parse::<kw::at_least>()?;
            Kind::AtLeast
        } else {
            input.parse::<kw::exactly>()?;
            Kind::Exactly
        };
        let inner;
        parenthesized!(inner in input);
        let threshold: LitInt = inner.parse()?;
        let threshold = threshold.base10_parse::<usize>()?;
        let mut predicates = Vec::new();
        while !inner.is_empty() {
            inner.parse::<Token![,]>()?;
            predicates.push(inner.parse::<Ident>()?);
        }
        if predicates.is_empty() {
            return Err(syn::Error::new(
                inner.span(),
                "cardinality constraint requires at least one predicate alias",
            ));
        }
        input.parse::<Token![;]>()?;
        return Ok(match kind {
            Kind::AtMost => ElasticPolicyConstraintDsl::AtMost {
                maximum: threshold,
                predicates,
            },
            Kind::AtLeast => ElasticPolicyConstraintDsl::AtLeast {
                minimum: threshold,
                predicates,
            },
            Kind::Exactly => ElasticPolicyConstraintDsl::Exactly {
                exact: threshold,
                predicates,
            },
        });
    }
    if input.peek(kw::requires) || input.peek(kw::equivalent) {
        let equivalent = input.peek(kw::equivalent);
        if equivalent {
            input.parse::<kw::equivalent>()?;
        } else {
            input.parse::<kw::requires>()?;
        }
        let inner;
        parenthesized!(inner in input);
        let left: Ident = inner.parse()?;
        inner.parse::<Token![,]>()?;
        let right: Ident = inner.parse()?;
        expect_exhausted(&inner, if equivalent { "equivalent" } else { "requires" })?;
        input.parse::<Token![;]>()?;
        return Ok(if equivalent {
            ElasticPolicyConstraintDsl::Equivalent { left, right }
        } else {
            ElasticPolicyConstraintDsl::Requires {
                feature: left,
                required: right,
            }
        });
    }
    if input.peek(kw::budget) {
        input.parse::<kw::budget>()?;
        let content;
        braced!(content in input);
        let mut unit = None;
        let mut quantum = None;
        let mut maximum = None;
        let mut terms = Vec::new();
        while !content.is_empty() {
            if content.peek(kw::unit) {
                content.parse::<kw::unit>()?;
                let inner;
                parenthesized!(inner in content);
                let value: LitStr = inner.parse()?;
                expect_exhausted(&inner, "unit")?;
                content.parse::<Token![;]>()?;
                if unit.replace(value.value()).is_some() {
                    return Err(syn::Error::new(
                        value.span(),
                        "constraint budget unit(...) declared more than once",
                    ));
                }
                continue;
            }
            if content.peek(kw::quantum) {
                content.parse::<kw::quantum>()?;
                let inner;
                parenthesized!(inner in content);
                let value: LitInt = inner.parse()?;
                expect_exhausted(&inner, "quantum")?;
                content.parse::<Token![;]>()?;
                if quantum.replace(value.base10_parse::<u64>()?).is_some() {
                    return Err(syn::Error::new(
                        value.span(),
                        "constraint budget quantum(...) declared more than once",
                    ));
                }
                continue;
            }
            if content.peek(kw::maximum) {
                content.parse::<kw::maximum>()?;
                let inner;
                parenthesized!(inner in content);
                let value = parse_signed_i128(&inner)?;
                expect_exhausted(&inner, "maximum")?;
                content.parse::<Token![;]>()?;
                if maximum.replace(value).is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "constraint budget maximum(...) declared more than once",
                    ));
                }
                continue;
            }
            if content.peek(kw::term) {
                content.parse::<kw::term>()?;
                let inner;
                parenthesized!(inner in content);
                let alias: Ident = inner.parse()?;
                inner.parse::<Token![,]>()?;
                let weight = parse_signed_i128(&inner)?;
                expect_exhausted(&inner, "term")?;
                content.parse::<Token![;]>()?;
                terms.push((alias, weight));
                continue;
            }
            return Err(syn::Error::new(
                content.span(),
                "constraint budget expects unit, quantum, maximum, or term",
            ));
        }
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        }
        if terms.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "constraint budget requires at least one term",
            ));
        }
        return Ok(ElasticPolicyConstraintDsl::Budget {
            unit: unit.ok_or_else(|| {
                syn::Error::new(input.span(), "constraint budget is missing unit(...)")
            })?,
            quantum: quantum.ok_or_else(|| {
                syn::Error::new(input.span(), "constraint budget is missing quantum(...)")
            })?,
            maximum: maximum.ok_or_else(|| {
                syn::Error::new(input.span(), "constraint budget is missing maximum(...)")
            })?,
            terms,
        });
    }
    Err(syn::Error::new(
        input.span(),
        "unsupported constraint; expected at_most, at_least, exactly, requires, equivalent, or budget",
    ))
}

fn validate_policy_alias_references(
    guards: &[ElasticPolicyGuardDsl],
    constraints: &[ElasticPolicyConstraintDsl],
    aliases: &std::collections::BTreeSet<String>,
) -> syn::Result<()> {
    for guard in guards {
        validate_guard_expression_aliases(&guard.expression, aliases)?;
    }
    let check = |alias: &Ident| -> syn::Result<()> {
        if aliases.contains(&alias.to_string()) {
            Ok(())
        } else {
            Err(syn::Error::new(
                alias.span(),
                format!("policy references undeclared predicate alias `{alias}`"),
            ))
        }
    };
    for constraint in constraints {
        match constraint {
            ElasticPolicyConstraintDsl::AtMost { predicates, .. }
            | ElasticPolicyConstraintDsl::AtLeast { predicates, .. }
            | ElasticPolicyConstraintDsl::Exactly { predicates, .. } => {
                for alias in predicates {
                    check(alias)?;
                }
            }
            ElasticPolicyConstraintDsl::Requires { feature, required } => {
                check(feature)?;
                check(required)?;
            }
            ElasticPolicyConstraintDsl::Equivalent { left, right } => {
                check(left)?;
                check(right)?;
            }
            ElasticPolicyConstraintDsl::Budget { terms, .. } => {
                for (alias, _) in terms {
                    check(alias)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_guard_expression_aliases(
    expression: &proc_macro2::TokenStream,
    aliases: &std::collections::BTreeSet<String>,
) -> syn::Result<()> {
    use proc_macro2::TokenTree;
    for token in expression.clone() {
        match token {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                if !aliases.contains(&name)
                    && !matches!(name.as_str(), "true" | "false" | "implies")
                {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("policy guard references undeclared predicate alias `{ident}`"),
                    ));
                }
            }
            TokenTree::Group(group) => validate_guard_expression_aliases(&group.stream(), aliases)?,
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
    Ok(())
}

fn validate_policy_references(
    policies: &[ElasticPolicyDsl],
    resources: &std::collections::BTreeSet<String>,
) -> syn::Result<()> {
    for policy in policies {
        if !resources.contains(&policy.target.to_string()) {
            return Err(syn::Error::new(
                policy.target.span(),
                format!(
                    "policy `{}` targets unknown document resource `{}`",
                    policy.name, policy.target
                ),
            ));
        }
    }
    Ok(())
}

fn parse_group_dsl(input: ParseStream<'_>) -> syn::Result<ElasticGroupDsl> {
    input.parse::<kw::group>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    let mut members: Option<Vec<Ident>> = None;
    let mut dependencies = Vec::new();
    let mut budgets = Vec::new();
    let mut invariants = Vec::new();
    let mut budget_names = std::collections::BTreeSet::new();

    while !content.is_empty() {
        if content.peek(kw::members) {
            let keyword: kw::members = content.parse()?;
            if members.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "group members(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let parsed = Punctuated::<Ident, Token![,]>::parse_terminated(&inner)?;
            if parsed.is_empty() {
                return Err(syn::Error::new(
                    inner.span(),
                    "group members(...) must not be empty",
                ));
            }
            content.parse::<Token![;]>()?;
            members = Some(parsed.into_iter().collect());
            continue;
        }
        if content.peek(kw::depends) {
            content.parse::<kw::depends>()?;
            let inner;
            parenthesized!(inner in content);
            let dependent: Ident = inner.parse()?;
            inner.parse::<Token![->]>()?;
            let required: Ident = inner.parse()?;
            expect_exhausted(&inner, "depends")?;
            content.parse::<Token![;]>()?;
            dependencies.push(ElasticGroupDependencyDsl {
                dependent,
                required,
            });
            continue;
        }
        if content.peek(kw::budget) {
            let budget = parse_budget_dsl(&content)?;
            if !budget_names.insert(budget.name.to_string()) {
                return Err(syn::Error::new(
                    budget.name.span(),
                    format!("duplicate budget `{}` in resource group", budget.name),
                ));
            }
            budgets.push(budget);
            continue;
        }
        if content.peek(kw::invariant) {
            invariants.push(parse_invariant_dsl(&content)?);
            continue;
        }
        return Err(syn::Error::new(
            content.span(),
            "expected members(...), depends(...), budget NAME { ... }, or invariant(...) in resource group",
        ));
    }

    let members = members
        .ok_or_else(|| syn::Error::new(name.span(), "resource group must declare members(...)"))?;
    Ok(ElasticGroupDsl {
        name,
        members,
        dependencies,
        budgets,
        invariants,
    })
}

fn parse_budget_dsl(input: ParseStream<'_>) -> syn::Result<ElasticBudgetDsl> {
    input.parse::<kw::budget>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    let mut unit = None;
    let mut quantum = None;
    let mut maximum = None;
    let mut terms = Vec::new();
    while !content.is_empty() {
        if content.peek(kw::unit) {
            let keyword: kw::unit = content.parse()?;
            if unit.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "budget unit(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let value: LitStr = inner.parse()?;
            expect_exhausted(&inner, "unit")?;
            content.parse::<Token![;]>()?;
            unit = Some(value.value());
            continue;
        }
        if content.peek(kw::quantum) {
            let keyword: kw::quantum = content.parse()?;
            if quantum.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "budget quantum(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let value: LitInt = inner.parse()?;
            expect_exhausted(&inner, "quantum")?;
            content.parse::<Token![;]>()?;
            quantum = Some(value.base10_parse::<u64>()?);
            continue;
        }
        if content.peek(kw::maximum) {
            let keyword: kw::maximum = content.parse()?;
            if maximum.is_some() {
                return Err(syn::Error::new(
                    keyword.span(),
                    "budget maximum(...) may be declared only once",
                ));
            }
            let inner;
            parenthesized!(inner in content);
            let value = parse_signed_i128(&inner)?;
            expect_exhausted(&inner, "maximum")?;
            content.parse::<Token![;]>()?;
            maximum = Some(value);
            continue;
        }
        if content.peek(kw::term) {
            content.parse::<kw::term>()?;
            let inner;
            parenthesized!(inner in content);
            let resource: Ident = inner.parse()?;
            inner.parse::<Token![,]>()?;
            inner.parse::<kw::predicate>()?;
            let predicate;
            parenthesized!(predicate in inner);
            let namespace: LitStr = predicate.parse()?;
            predicate.parse::<Token![,]>()?;
            let predicate_name: LitStr = predicate.parse()?;
            expect_exhausted(&predicate, "predicate")?;
            inner.parse::<Token![,]>()?;
            let weight = parse_signed_i128(&inner)?;
            expect_exhausted(&inner, "term")?;
            content.parse::<Token![;]>()?;
            terms.push(ElasticBudgetTermDsl {
                resource,
                predicate_namespace: namespace.value(),
                predicate_name: predicate_name.value(),
                weight,
            });
            continue;
        }
        return Err(syn::Error::new(
            content.span(),
            "expected unit(...), quantum(...), maximum(...), or term(...) in shared budget",
        ));
    }
    if terms.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "shared budget must contain at least one term(...)",
        ));
    }
    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
    }
    let name_span = name.span();
    Ok(ElasticBudgetDsl {
        name,
        unit: unit
            .ok_or_else(|| syn::Error::new(name_span, "shared budget is missing unit(...)"))?,
        quantum: quantum
            .ok_or_else(|| syn::Error::new(name_span, "shared budget is missing quantum(...)"))?,
        maximum: maximum
            .ok_or_else(|| syn::Error::new(name_span, "shared budget is missing maximum(...)"))?,
        terms,
    })
}

fn parse_invariant_dsl(input: ParseStream<'_>) -> syn::Result<ElasticInvariantDsl> {
    input.parse::<kw::invariant>()?;
    let content;
    parenthesized!(content in input);
    content.parse::<kw::contract>()?;
    let contract_inner;
    parenthesized!(contract_inner in content);
    let contract: LitStr = contract_inner.parse()?;
    expect_exhausted(&contract_inner, "contract")?;
    content.parse::<Token![,]>()?;
    content.parse::<kw::owner>()?;
    let owner_inner;
    parenthesized!(owner_inner in content);
    let owner: Ident = owner_inner.parse()?;
    expect_exhausted(&owner_inner, "owner")?;
    content.parse::<Token![,]>()?;
    content.parse::<kw::participants>()?;
    let participants_inner;
    parenthesized!(participants_inner in content);
    let participants = Punctuated::<Ident, Token![,]>::parse_terminated(&participants_inner)?;
    if participants.is_empty() {
        return Err(syn::Error::new(
            participants_inner.span(),
            "invariant participants(...) must not be empty",
        ));
    }
    expect_exhausted(&content, "invariant")?;
    input.parse::<Token![;]>()?;
    Ok(ElasticInvariantDsl {
        contract: contract.value(),
        owner,
        participants: participants.into_iter().collect(),
    })
}

fn parse_signed_i128(input: ParseStream<'_>) -> syn::Result<i128> {
    let negative = if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        true
    } else {
        false
    };
    let value: LitInt = input.parse()?;
    let magnitude = value.base10_parse::<i128>()?;
    if negative {
        magnitude
            .checked_neg()
            .ok_or_else(|| syn::Error::new(value.span(), "signed integer is outside i128 range"))
    } else {
        Ok(magnitude)
    }
}

fn validate_group_references(
    groups: &[ElasticGroupDsl],
    resources: &std::collections::BTreeSet<String>,
) -> syn::Result<()> {
    let check = |ident: &Ident| {
        if resources.contains(&ident.to_string()) {
            Ok(())
        } else {
            Err(syn::Error::new(
                ident.span(),
                format!("resource group references unknown document resource `{ident}`"),
            ))
        }
    };
    for group in groups {
        for member in &group.members {
            check(member)?;
        }
        for dependency in &group.dependencies {
            check(&dependency.dependent)?;
            check(&dependency.required)?;
        }
        for budget in &group.budgets {
            for term in &budget.terms {
                check(&term.resource)?;
            }
        }
        for invariant in &group.invariants {
            check(&invariant.owner)?;
            for participant in &invariant.participants {
                check(participant)?;
            }
        }
    }
    Ok(())
}

fn expand_elastic_dsl(input: ElasticDslInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    match input {
        ElasticDslInput::Resource(resource) => expand_resource_module(resource),
        ElasticDslInput::Document(document) => expand_document_module(document),
    }
}

fn expand_resource_module(
    input: ElasticResourceDsl,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ElasticResourceDsl {
        visibility,
        name,
        body,
    } = input;
    let entries = split_dsl_entries(body)?;
    if entries.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "elastic! resource body must contain class(...) and at least one allow(...) declaration",
        ));
    }

    let normalized = quote! { #(#entries),* };
    let parsed = Punctuated::<Entry, Token![,]>::parse_terminated.parse2(normalized.clone())?;
    let has_explicit_id = parsed
        .iter()
        .any(|entry| matches!(entry.kind, EntryKind::Id(_)));
    analyze_entries(parsed, name.span())?;
    let default_id = LitStr::new(&name.to_string(), name.span());
    let helper_args = if has_explicit_id {
        normalized
    } else {
        quote! { id(#default_id), #normalized }
    };

    Ok(quote! {
        #visibility mod #name {
            #[derive(::elastic::ElasticResource)]
            #[elastic(#helper_args)]
            struct __ElasticDeclaration;

            #[doc = concat!(
                "Returns the validated [`ResourceSpec`](::elastic::resource::ResourceSpec) ",
                "declared by `elastic!` resource `",
                stringify!(#name),
                "`."
            )]
            pub fn resource_spec()
                -> ::core::result::Result<
                    ::elastic::resource::ResourceSpec,
                    ::elastic::resource::ResourceSpecError,
                > {
                __ElasticDeclaration::resource_spec()
            }
        }
    })
}

fn expand_policy_module(policy: ElasticPolicyDsl) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ElasticPolicyDsl {
        name,
        id,
        version: (major, minor, patch),
        target,
        predicates,
        guards,
        constraints,
        objectives,
        hints,
    } = policy;
    let id_lit = LitStr::new(&id, name.span());

    let predicate_bindings = predicates.iter().map(|predicate| {
        let alias = &predicate.alias;
        let namespace = LitStr::new(&predicate.namespace, predicate.alias.span());
        let predicate_name = LitStr::new(&predicate.name, predicate.alias.span());
        quote! {
            let #alias = ::elastic::PredicateKey::new(#namespace, #predicate_name)?;
        }
    });
    let aliases = predicates
        .iter()
        .map(|predicate| predicate.alias.clone())
        .collect::<Vec<_>>();
    let predicate_registry = if aliases.is_empty() {
        quote! { let __elastic_predicates = ::elastic::ElasticPredicates::empty(); }
    } else {
        quote! {
            let __elastic_predicates = ::elastic::ElasticPredicates::new([
                #(#aliases.clone()),*
            ])?;
        }
    };

    let guard_exprs = guards.into_iter().map(|guard| {
        let scope = match guard.scope {
            ElasticPolicyGuardScopeDsl::Resource => quote! { ::elastic::GuardScope::Resource },
            ElasticPolicyGuardScopeDsl::Dimension(dimension) => {
                let dimension = term_expr(&dimension, TermPath::Dimension);
                quote! { ::elastic::GuardScope::Dimension(#dimension) }
            }
            ElasticPolicyGuardScopeDsl::Transition {
                mechanism,
                dimension,
            } => {
                let mechanism = Ident::new(mechanism, Span::call_site());
                let dimension = term_expr(&dimension, TermPath::Dimension);
                quote! {
                    ::elastic::GuardScope::Transition {
                        mechanism: ::elastic::TransitionMechanism::#mechanism,
                        dimension: #dimension,
                    }
                }
            }
        };
        let expression = guard.expression;
        quote! {
            ::elastic::elastic_guard! {
                predicates: __elastic_predicates,
                scope: #scope,
                when: (#expression),
            }?
        }
    });

    let constraint_exprs = constraints.into_iter().map(|constraint| match constraint {
        ElasticPolicyConstraintDsl::AtMost {
            maximum,
            predicates,
        } => quote! {
            ::elastic::PseudoBooleanConstraintDeclaration::at_most_keys(
                [#(#predicates.clone()),*],
                #maximum,
            )?
        },
        ElasticPolicyConstraintDsl::AtLeast {
            minimum,
            predicates,
        } => quote! {
            ::elastic::PseudoBooleanConstraintDeclaration::at_least_keys(
                [#(#predicates.clone()),*],
                #minimum,
            )?
        },
        ElasticPolicyConstraintDsl::Exactly { exact, predicates } => quote! {
            ::elastic::PseudoBooleanConstraintDeclaration::exactly_keys(
                [#(#predicates.clone()),*],
                #exact,
            )?
        },
        ElasticPolicyConstraintDsl::Requires { feature, required } => quote! {
            ::elastic::PseudoBooleanConstraintDeclaration::requires_key(
                #feature.clone(),
                #required.clone(),
            )?
        },
        ElasticPolicyConstraintDsl::Equivalent { left, right } => quote! {
            ::elastic::PseudoBooleanConstraintDeclaration::equivalent_keys(
                #left.clone(),
                #right.clone(),
            )?
        },
        ElasticPolicyConstraintDsl::Budget {
            unit,
            quantum,
            maximum,
            terms,
        } => {
            let unit = LitStr::new(&unit, name.span());
            let terms = terms.into_iter().map(|(alias, weight)| {
                quote! {
                    ::elastic::WeightedPredicateKey::new(#alias.clone(), #weight)?
                }
            });
            quote! {
                ::elastic::PseudoBooleanConstraintDeclaration::capacity_budget(
                    vec![#(#terms),*],
                    #maximum,
                    ::elastic::PseudoBooleanScale::new(#unit, #quantum)?,
                )?
            }
        }
    });

    let objective_exprs = objectives.into_iter().map(|objective| {
        let objective_term = term_expr(&objective.objective, TermPath::Objective);
        let direction = Ident::new(objective.direction, Span::call_site());
        let unit = LitStr::new(&objective.unit, name.span());
        let quantum = objective.quantum;
        quote! {
            ::elastic::PolicyNumericObjective::new(
                #objective_term,
                ::elastic::PolicyObjectiveDirection::#direction,
                ::elastic::PolicyMetricScale::new(#unit, #quantum)?,
            )
        }
    });
    let hint_exprs = hints.into_iter().map(|(key, value)| {
        let key = LitStr::new(&key, name.span());
        let value = LitStr::new(&value, name.span());
        quote! {
            ::elastic::PlannerHint::new(
                ::elastic::PlannerHintKey::new(#key)?,
                #value,
            )?
        }
    });

    Ok(quote! {
        pub mod #name {
            /// Build the typed ELANG5 resource policy declared by this module.
            pub fn policy_spec()
                -> ::core::result::Result<
                    ::elastic::ResourcePolicyAdvisorySpec,
                    ::elastic::ElasticPolicyDocumentError,
                > {
                let __elastic_resource = super::#target::resource_spec()
                    .map_err(|source| {
                        ::elastic::ElasticPolicyDocumentError::resource(
                            stringify!(#target),
                            source,
                        )
                    })?;
                #(#predicate_bindings)*
                #predicate_registry
                let __elastic_header = ::elastic::PolicyHeader::new(
                    ::elastic::PolicyIdentity::new(
                        ::elastic::PolicyId::new(#id_lit)?,
                        ::elastic::PolicyVersion::new(#major, #minor, #patch),
                    ),
                    ::elastic::PolicyTarget::resource(
                        __elastic_resource.resource_id().clone(),
                    ),
                );
                let __elastic_policy = ::elastic::ResourcePolicySpec::new(
                    __elastic_header,
                    __elastic_resource,
                    vec![#(#guard_exprs),*],
                    vec![#(#constraint_exprs),*],
                )?;
                ::elastic::ResourcePolicyAdvisorySpec::new(
                    __elastic_policy,
                    vec![#(#objective_exprs),*],
                    vec![#(#hint_exprs),*],
                )
                .map_err(::core::convert::Into::into)
            }

            /// Lower the typed policy through the existing ELANG5 EIR authority.
            pub fn policy_eir()
                -> ::core::result::Result<
                    ::elastic::EirResourcePolicyAdvisory,
                    ::elastic::ElasticPolicyDocumentError,
                > {
                let policy = policy_spec()?;
                ::elastic::lower_resource_policy_advisory(&policy)
                    .map_err(::core::convert::Into::into)
            }
        }
    })
}

fn expand_document_module(
    input: ElasticDocumentDsl,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ElasticDocumentDsl {
        visibility,
        name,
        resources,
        groups,
        policies,
    } = input;
    let resource_count = resources.len();
    let group_count = groups.len();
    let mut resource_modules = Vec::with_capacity(resource_count);
    let mut resource_names = Vec::with_capacity(resource_count);
    let mut resource_ids = std::collections::BTreeMap::new();
    for resource in resources {
        let resource_name = resource.name.clone();
        let id_var = format_ident!("__elastic_id_{}", resource_name);
        resource_ids.insert(resource_name.to_string(), id_var);
        resource_modules.push(expand_resource_module(ElasticResourceDsl {
            visibility: syn::parse_quote!(pub),
            name: resource.name,
            body: resource.body,
        })?);
        resource_names.push(resource_name);
    }

    let mut policy_modules = Vec::with_capacity(policies.len());
    for policy in policies {
        policy_modules.push(expand_policy_module(policy)?);
    }

    let grouped_function = if groups.is_empty() {
        quote! {}
    } else {
        let id_bindings = resource_names.iter().map(|resource_name| {
            let id_var = &resource_ids[&resource_name.to_string()];
            quote! {
                let #id_var = #resource_name::resource_spec()
                    .map_err(|source| {
                        ::elastic::ElasticGroupDocumentError::from(
                            ::elastic::ElasticDocumentError::resource(
                                stringify!(#resource_name),
                                source,
                            )
                        )
                    })?
                    .resource_id()
                    .clone();
            }
        });

        let mut group_exprs = Vec::with_capacity(groups.len());
        for group in groups {
            let group_name = group.name.to_string();
            let group_name_lit = LitStr::new(&group_name, group.name.span());
            let member_vars = group
                .members
                .iter()
                .map(|member| resource_ids[&member.to_string()].clone())
                .collect::<Vec<_>>();
            let dependency_suffixes = group.dependencies.iter().map(|dependency| {
                let dependent = &resource_ids[&dependency.dependent.to_string()];
                let required = &resource_ids[&dependency.required.to_string()];
                quote! {
                    .dependency(::elastic::ResourceDependency::new(
                        #dependent.clone(),
                        #required.clone(),
                    ))
                }
            });

            let mut budget_statements = Vec::with_capacity(group.budgets.len());
            for budget in group.budgets {
                let budget_name = LitStr::new(&budget.name.to_string(), budget.name.span());
                let unit = LitStr::new(&budget.unit, budget.name.span());
                let quantum = budget.quantum;
                let maximum = budget.maximum;
                let terms = budget.terms.iter().map(|term| {
                    let resource = &resource_ids[&term.resource.to_string()];
                    let namespace = LitStr::new(&term.predicate_namespace, term.resource.span());
                    let predicate_name = LitStr::new(&term.predicate_name, term.resource.span());
                    let weight = term.weight;
                    quote! {
                        ::elastic::SharedBudgetTerm::new(
                            #resource.clone(),
                            ::elastic::PredicateKey::new(#namespace, #predicate_name)?,
                            #weight,
                        )?
                    }
                });
                budget_statements.push(quote! {
                    __elastic_group_builder = __elastic_group_builder.shared_budget(
                        ::elastic::SharedBudget::new(
                            ::elastic::SharedBudgetId::new(#budget_name)?,
                            vec![#(#terms),*],
                            #maximum,
                            ::elastic::PseudoBooleanScale::new(#unit, #quantum)?,
                        )?
                    );
                });
            }

            let mut invariant_statements = Vec::with_capacity(group.invariants.len());
            for invariant in group.invariants {
                let contract = LitStr::new(&invariant.contract, invariant.owner.span());
                let owner = &resource_ids[&invariant.owner.to_string()];
                let participants = invariant
                    .participants
                    .iter()
                    .map(|participant| resource_ids[&participant.to_string()].clone())
                    .collect::<Vec<_>>();
                invariant_statements.push(quote! {
                    __elastic_group_builder = __elastic_group_builder.cross_invariant(
                        ::elastic::CrossResourceInvariant::new(
                            ::elastic::ContractId::new(#contract)?,
                            #owner.clone(),
                            vec![#(#participants.clone()),*],
                        )?
                    );
                });
            }

            group_exprs.push(quote! {
                {
                    let mut __elastic_group_builder = ::elastic::ResourceGroupBuilder::new(
                        ::elastic::ResourceGroupId::new(#group_name_lit)?,
                    )
                    .members(vec![#(#member_vars.clone()),*])
                    #(#dependency_suffixes)*;
                    #(#budget_statements)*
                    #(#invariant_statements)*
                    __elastic_group_builder.build()?
                }
            });
        }

        quote! {
            #[doc = concat!(
                "Builds the validated grouped [`EirGroupedDocument`](::elastic::EirGroupedDocument) ",
                "declared by `elastic!` document `",
                stringify!(#name),
                "`."
            )]
            pub fn grouped_document()
                -> ::core::result::Result<
                    ::elastic::EirGroupedDocument,
                    ::elastic::ElasticGroupDocumentError,
                > {
                let __elastic_document = document()?;
                #(#id_bindings)*
                let __elastic_groups = vec![#(#group_exprs),*];
                ::elastic::EirGroupedDocument::new(__elastic_document, &__elastic_groups)
                    .map_err(::core::convert::Into::into)
            }
        }
    };

    Ok(quote! {
        #visibility mod #name {
            const _: () = {
                assert!(
                    #resource_count <= ::elastic::MAX_EIR_DOCUMENT_RESOURCES,
                    "elastic! document exceeds MAX_EIR_DOCUMENT_RESOURCES",
                );
                assert!(
                    #group_count <= ::elastic::MAX_EIR_RESOURCE_GROUPS,
                    "elastic! document exceeds MAX_EIR_RESOURCE_GROUPS",
                );
            };

            #(#resource_modules)*
            #(#policy_modules)*

            #[doc = concat!(
                "Builds the validated multi-resource [`EirDocument`](::elastic::EirDocument) ",
                "declared by `elastic!` document `",
                stringify!(#name),
                "`."
            )]
            pub fn document()
                -> ::core::result::Result<
                    ::elastic::EirDocument,
                    ::elastic::ElasticDocumentError,
                > {
                let mut builder = ::elastic::EirDocumentBuilder::new();
                #(
                    let spec = #resource_names::resource_spec().map_err(|source| {
                        ::elastic::ElasticDocumentError::resource(
                            stringify!(#resource_names),
                            source,
                        )
                    })?;
                    builder.push(&spec)?;
                )*
                builder.finish().map_err(::core::convert::Into::into)
            }

            #grouped_function
        }
    })
}

fn split_dsl_entries(
    body: proc_macro2::TokenStream,
) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    use proc_macro2::{TokenStream as TokenStream2, TokenTree};

    let mut entries = Vec::new();
    let mut current = TokenStream2::new();
    for tree in body {
        match &tree {
            TokenTree::Punct(punct) if punct.as_char() == ';' => {
                if current.is_empty() {
                    return Err(syn::Error::new(
                        punct.span(),
                        "empty elastic! declaration; remove the extra `;`",
                    ));
                }
                entries.push(current);
                current = TokenStream2::new();
            }
            TokenTree::Punct(punct) if punct.as_char() == ',' => {
                return Err(syn::Error::new(
                    punct.span(),
                    "use `;` between elastic! resource declarations (commas remain valid inside (...) payloads)",
                ));
            }
            _ => current.extend([tree]),
        }
    }
    if !current.is_empty() {
        entries.push(current);
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Vocabulary tables: `(accepted identifier, generated constant)` pairs.
// ---------------------------------------------------------------------------

const DIMENSIONS: &[(&str, &str)] = &[
    ("capacity", "CAPACITY"),
    ("concurrency", "CONCURRENCY"),
    ("residency", "RESIDENCY"),
    ("locality", "LOCALITY"),
    ("representation", "REPRESENTATION"),
    ("precision", "PRECISION"),
    ("parallelism", "PARALLELISM"),
    ("routing", "ROUTING"),
    ("redundancy", "REDUNDANCY"),
    ("persistence", "PERSISTENCE"),
    ("recomputability", "RECOMPUTABILITY"),
    ("bandwidth", "BANDWIDTH"),
    ("energy", "ENERGY"),
];

const OBJECTIVES: &[(&str, &str)] = &[
    ("latency", "LATENCY"),
    ("throughput", "THROUGHPUT"),
    ("memory-footprint", "MEMORY_FOOTPRINT"),
    ("memory_footprint", "MEMORY_FOOTPRINT"),
    ("energy", "ENERGY"),
    ("migration-cost", "MIGRATION_COST"),
    ("migration_cost", "MIGRATION_COST"),
    ("stability", "STABILITY"),
];

const CLASSES: &[(&str, &str)] = &[
    ("stock", "STOCK"),
    ("capacity-resource", "CAPACITY_RESOURCE"),
    ("capacity_resource", "CAPACITY_RESOURCE"),
    ("rate", "RATE"),
    ("exclusive", "EXCLUSIVE"),
    ("shared", "SHARED"),
    ("stateful", "STATEFUL"),
    ("representational", "REPRESENTATIONAL"),
    ("configurational", "CONFIGURATIONAL"),
];

const SIGNALS: &[(&str, &str)] = &[
    ("free-capacity", "FREE_CAPACITY"),
    ("free_capacity", "FREE_CAPACITY"),
    ("utilization", "UTILIZATION"),
    ("queue-depth", "QUEUE_DEPTH"),
    ("queue_depth", "QUEUE_DEPTH"),
    ("latency-sample", "LATENCY_SAMPLE"),
    ("latency_sample", "LATENCY_SAMPLE"),
    ("thermal-margin", "THERMAL_MARGIN"),
    ("thermal_margin", "THERMAL_MARGIN"),
    ("energy-rate", "ENERGY_RATE"),
    ("energy_rate", "ENERGY_RATE"),
    ("topology-change", "TOPOLOGY_CHANGE"),
    ("topology_change", "TOPOLOGY_CHANGE"),
];

/// `(accepted identifier, generated enum variant)` pairs.
const MECHANISMS: &[(&str, &str)] = &[
    ("reinterpret", "Reinterpret"),
    ("reencode", "Reencode"),
    ("recompute", "Recompute"),
];

const KNOWN_KEYS: &[&str] = &[
    "class",
    "id",
    "allow",
    "preserve",
    "optimize",
    "admit",
    "capability",
    "observe",
    "label",
];

// ---------------------------------------------------------------------------
// Parsed declaration model
// ---------------------------------------------------------------------------

enum ClassRef {
    Builtin(&'static str),
    Custom(String),
}

enum TermRef {
    Builtin(&'static str),
    Custom(String),
}

enum PreserveRef {
    Contents,
    Identity,
    Contract(String),
}

enum Fragment {
    Allow(Vec<TermRef>),
    Preserve {
        kind: PreserveRef,
        along: Option<TermRef>,
    },
    Optimize(Vec<TermRef>),
    Admit {
        mechanism: &'static str,
        dimension: TermRef,
    },
    Capability {
        mechanism: &'static str,
        dimension: TermRef,
    },
    Observe(Vec<TermRef>),
    Label(String, String),
}

struct Entry {
    span: Span,
    kind: EntryKind,
}

enum EntryKind {
    Class(ClassRef),
    Id(String),
    Fragment(Fragment),
}

struct ParsedDeclaration {
    class_ref: ClassRef,
    id: Option<String>,
    fragments: Vec<Fragment>,
}

fn analyze_entries(
    entries: impl IntoIterator<Item = Entry>,
    owner_span: Span,
) -> Result<ParsedDeclaration, syn::Error> {
    let mut class_ref: Option<ClassRef> = None;
    let mut id: Option<String> = None;
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut combined_error: Option<syn::Error> = None;

    for entry in entries {
        match entry.kind {
            EntryKind::Class(class) => {
                if class_ref.is_some() {
                    combined_error = combine(
                        combined_error,
                        syn::Error::new(
                            entry.span,
                            "duplicate mutually exclusive key `class`; declare it once",
                        ),
                    );
                } else {
                    class_ref = Some(class);
                }
            }
            EntryKind::Id(value) => {
                if id.is_some() {
                    combined_error = combine(
                        combined_error,
                        syn::Error::new(
                            entry.span,
                            "duplicate mutually exclusive key `id`; declare it once",
                        ),
                    );
                } else {
                    id = Some(value);
                }
            }
            EntryKind::Fragment(fragment) => fragments.push(fragment),
        }
    }

    if let Some(error) = combined_error {
        return Err(error);
    }
    let class_ref = class_ref.ok_or_else(|| {
        syn::Error::new(
            owner_span,
            "missing mandatory `class(...)` declaration; expected one of \
             stock, capacity-resource, rate, exclusive, shared, stateful, \
             representational, configurational, or class(custom(\"...\"))",
        )
    })?;
    let has_elasticity = fragments
        .iter()
        .any(|fragment| matches!(fragment, Fragment::Allow(terms) if !terms.is_empty()));
    if !has_elasticity {
        return Err(syn::Error::new(
            owner_span,
            "missing mandatory elasticity: declare at least one allow(...) dimension",
        ));
    }

    Ok(ParsedDeclaration {
        class_ref,
        id,
        fragments,
    })
}

// ---------------------------------------------------------------------------
// Expansion
// ---------------------------------------------------------------------------

fn expand(input: &DeriveInput) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ident = &input.ident;
    if !matches!(input.data, Data::Struct(_)) {
        return Err(syn::Error::new(
            input.span(),
            "#[derive(ElasticResource)] only supports structs",
        ));
    }

    let elastic_attrs: Vec<&Attribute> = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("elastic"))
        .collect();
    if elastic_attrs.is_empty() {
        return Err(syn::Error::new(
            input.span(),
            "missing #[elastic(...)] attribute; declare at least class(...) and allow(...)",
        ));
    }

    let mut entries = Vec::new();
    for attr in &elastic_attrs {
        entries.extend(attr.parse_args_with(Punctuated::<Entry, Token![,]>::parse_terminated)?);
    }
    let ParsedDeclaration {
        class_ref,
        id,
        fragments,
    } = analyze_entries(entries, ident.span())?;

    let id_lit = match &id {
        Some(text) => LitStr::new(text, ident.span()),
        // Default identity: the struct name itself, documented behavior.
        None => LitStr::new(&ident.to_string(), ident.span()),
    };

    let class_expr = match &class_ref {
        ClassRef::Builtin(const_name) => {
            let const_ident = Ident::new(const_name, Span::call_site());
            quote! { ::elastic::resource::ResourceClassId::#const_ident }
        }
        ClassRef::Custom(text) => quote! {
            ::elastic::resource::ResourceClassId::custom(#text)?
        },
    };

    let mut suffixes: Vec<proc_macro2::TokenStream> = Vec::new();
    for fragment in &fragments {
        match fragment {
            Fragment::Allow(terms) => {
                for term in terms {
                    let expr = term_expr(term, TermPath::Dimension);
                    suffixes.push(quote! { .allow(#expr) });
                }
            }
            Fragment::Optimize(terms) => {
                for term in terms {
                    let expr = term_expr(term, TermPath::Objective);
                    suffixes.push(quote! { .optimize(#expr) });
                }
            }
            Fragment::Observe(terms) => {
                for term in terms {
                    let expr = term_expr(term, TermPath::Signal);
                    suffixes.push(quote! { .observe(#expr) });
                }
            }
            Fragment::Preserve { kind, along } => {
                let base = match kind {
                    PreserveRef::Contents => quote! {
                        ::elastic::resource::Invariant::new(
                            ::elastic::resource::InvariantKind::PreserveContents,
                        )
                    },
                    PreserveRef::Identity => quote! {
                        ::elastic::resource::Invariant::new(
                            ::elastic::resource::InvariantKind::PreserveIdentity,
                        )
                    },
                    PreserveRef::Contract(text) => quote! {
                        ::elastic::resource::Invariant::new(
                            ::elastic::resource::InvariantKind::UpholdContract(
                                ::elastic::resource::ContractId::new(#text)?,
                            ),
                        )
                    },
                };
                let invariant = match along {
                    Some(dim) => {
                        let dim_expr = term_expr(dim, TermPath::Dimension);
                        quote! { #base.along(#dim_expr) }
                    }
                    None => base,
                };
                suffixes.push(quote! { .preserve(#invariant) });
            }
            Fragment::Admit {
                mechanism,
                dimension,
            } => {
                let variant = Ident::new(mechanism, Span::call_site());
                let dim_expr = term_expr(dimension, TermPath::Dimension);
                suffixes.push(quote! {
                    .admit(::elastic::resource::AdmissibleTransition::new(
                        ::elastic::TransitionMechanism::#variant,
                        #dim_expr,
                    ))
                });
            }
            Fragment::Capability {
                mechanism,
                dimension,
            } => {
                let variant = Ident::new(mechanism, Span::call_site());
                let dim_expr = term_expr(dimension, TermPath::Dimension);
                suffixes.push(quote! {
                    .require_capability(::elastic::resource::CapabilityRequirement::new(
                        ::elastic::TransitionMechanism::#variant,
                        #dim_expr,
                    ))
                });
            }
            Fragment::Label(key, value) => {
                suffixes.push(quote! { .label(#key, #value) });
            }
        }
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        const _: () = {
            #[doc = concat!(
                "Returns the validated ",
                "[`ResourceSpec`](::elastic::resource::ResourceSpec)",
                " declared by `",
                stringify!(#ident),
                "` via `#[derive(ElasticResource)]`."
            )]
            impl #impl_generics #ident #ty_generics #where_clause {
                pub fn resource_spec()
                    -> ::core::result::Result<
                        ::elastic::resource::ResourceSpec,
                        ::elastic::resource::ResourceSpecError,
                    > {
                    let builder = ::elastic::resource::ResourceSpec::builder(
                        #class_expr,
                        ::elastic::resource::LogicalResourceId::new(#id_lit)?,
                    )
                    #(#suffixes)*
                    ;
                    builder.build()
                }
            }
        };
    })
}

fn combine(first: Option<syn::Error>, second: syn::Error) -> Option<syn::Error> {
    Some(match first {
        Some(mut existing) => {
            existing.combine(second);
            existing
        }
        None => second,
    })
}

enum TermPath {
    Dimension,
    Objective,
    Signal,
}

fn term_expr(term: &TermRef, path: TermPath) -> proc_macro2::TokenStream {
    match term {
        TermRef::Builtin(const_name) => {
            let const_ident = Ident::new(const_name, Span::call_site());
            match path {
                TermPath::Dimension => {
                    quote! { ::elastic::resource::DimensionId::#const_ident }
                }
                TermPath::Objective => {
                    quote! { ::elastic::resource::ObjectiveId::#const_ident }
                }
                TermPath::Signal => {
                    quote! { ::elastic::resource::ObservationSignalId::#const_ident }
                }
            }
        }
        TermRef::Custom(text) => match path {
            TermPath::Dimension => {
                quote! { ::elastic::resource::DimensionId::custom(#text)? }
            }
            TermPath::Objective => {
                quote! { ::elastic::resource::ObjectiveId::custom(#text)? }
            }
            TermPath::Signal => {
                quote! { ::elastic::resource::ObservationSignalId::custom(#text)? }
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Attribute grammar
// ---------------------------------------------------------------------------

impl Parse for Entry {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        let key_span = key.span();
        let kind = match key.to_string().as_str() {
            "class" => {
                let content;
                parenthesized!(content in input);
                let parsed_class = parse_class(&content)?;
                expect_exhausted(&content, "class")?;
                EntryKind::Class(parsed_class)
            }
            "id" => {
                let content;
                parenthesized!(content in input);
                let value: LitStr = content.parse()?;
                expect_exhausted(&content, "id")?;
                EntryKind::Id(value.value())
            }
            "allow" => {
                let content;
                parenthesized!(content in input);
                let terms = parse_term_list(&content, "dimension", DIMENSIONS)?;
                if terms.is_empty() {
                    return Err(syn::Error::new(
                        key_span,
                        "allow(...) must declare at least one dimension",
                    ));
                }
                EntryKind::Fragment(Fragment::Allow(terms))
            }
            "optimize" => {
                let content;
                parenthesized!(content in input);
                let terms = parse_term_list(&content, "objective", OBJECTIVES)?;
                if terms.is_empty() {
                    return Err(syn::Error::new(
                        key_span,
                        "optimize(...) must declare at least one objective",
                    ));
                }
                EntryKind::Fragment(Fragment::Optimize(terms))
            }
            "observe" => {
                let content;
                parenthesized!(content in input);
                let terms = parse_term_list(&content, "observation signal", SIGNALS)?;
                if terms.is_empty() {
                    return Err(syn::Error::new(
                        key_span,
                        "observe(...) must declare at least one signal",
                    ));
                }
                EntryKind::Fragment(Fragment::Observe(terms))
            }
            "preserve" => {
                let content;
                parenthesized!(content in input);
                let kind = parse_preserve_kind(&content)?;
                // `along <dimension>` is consumed inside the branch below.
                let along = peek_keyword(&content, "along")?;
                let along = match along {
                    true => Some(parse_term(&content, "dimension", DIMENSIONS)?),
                    false => None,
                };
                expect_exhausted(&content, "preserve")?;
                EntryKind::Fragment(Fragment::Preserve { kind, along })
            }
            "admit" | "capability" => {
                let content;
                parenthesized!(content in input);
                let mechanism = parse_mechanism(&content)?;
                content.parse::<Token![@]>().map_err(|_| {
                    syn::Error::new(
                        content.span(),
                        format!(
                            "expected `<mechanism> @ <dimension>` after `{}` \
                             (mechanisms: {})",
                            key,
                            mech_names().join(", ")
                        ),
                    )
                })?;
                let dimension = parse_term(&content, "dimension", DIMENSIONS)?;
                expect_exhausted(&content, &key.to_string())?;
                let fragment = if key == "admit" {
                    Fragment::Admit {
                        mechanism,
                        dimension,
                    }
                } else {
                    Fragment::Capability {
                        mechanism,
                        dimension,
                    }
                };
                EntryKind::Fragment(fragment)
            }
            "label" => {
                let content;
                parenthesized!(content in input);
                let label_key: LitStr = content.parse()?;
                content.parse::<Token![,]>()?;
                let value: LitStr = content.parse()?;
                expect_exhausted(&content, "label")?;
                EntryKind::Fragment(Fragment::Label(label_key.value(), value.value()))
            }
            other => {
                return Err(syn::Error::new(
                    key_span,
                    format!(
                        "unknown `elastic` attribute key `{other}`; expected one of {}",
                        KNOWN_KEYS.join(", ")
                    ),
                ));
            }
        };
        Ok(Entry {
            span: key_span,
            kind,
        })
    }
}

/// Consume the bare identifier `keyword` when it is next in the stream.
fn peek_keyword(input: ParseStream<'_>, keyword: &str) -> syn::Result<bool> {
    if input.peek(Ident) {
        let fork = input.fork();
        let ident: Ident = fork.parse()?;
        if ident == keyword {
            let consumed: Ident = input.parse()?;
            let _ = consumed;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reject any token a payload parser did not consume, so malformed
/// declarations fail loudly instead of being silently truncated.
fn expect_exhausted(content: ParseStream<'_>, key: &str) -> syn::Result<()> {
    if content.is_empty() {
        Ok(())
    } else {
        Err(syn::Error::new(
            content.span(),
            format!("unexpected trailing tokens in `{key}(...)` payload"),
        ))
    }
}

fn mech_names() -> Vec<&'static str> {
    MECHANISMS.iter().map(|(name, _)| *name).collect()
}

/// Parse one term reference: a known identifier or `custom("...")`.
fn parse_term(
    input: ParseStream<'_>,
    kind: &str,
    table: &'static [(&'static str, &'static str)],
) -> syn::Result<TermRef> {
    if !input.peek(Ident) {
        return Err(syn::Error::new(
            input.span(),
            format!("expected a {kind} identifier or custom(\"...\")"),
        ));
    }
    let ident: Ident = input.parse()?;
    if ident == "custom" {
        let inner;
        parenthesized!(inner in input);
        let text: LitStr = inner.parse()?;
        expect_exhausted(&inner, "custom")?;
        return Ok(TermRef::Custom(text.value()));
    }
    lookup(table, &ident)
        .map(TermRef::Builtin)
        .ok_or_else(|| unknown_term_error(&ident, kind, builtin_keys(table)))
}

fn parse_term_list(
    input: ParseStream<'_>,
    kind: &str,
    table: &'static [(&'static str, &'static str)],
) -> syn::Result<Vec<TermRef>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        out.push(parse_term(input, kind, table)?);
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(out)
}

fn parse_class(input: ParseStream<'_>) -> syn::Result<ClassRef> {
    let term = parse_term(input, "resource class", CLASSES)?;
    Ok(match term {
        TermRef::Builtin(name) => ClassRef::Builtin(name),
        TermRef::Custom(text) => ClassRef::Custom(text),
    })
}

fn parse_preserve_kind(input: ParseStream<'_>) -> syn::Result<PreserveRef> {
    if !input.peek(Ident) {
        return Err(syn::Error::new(
            input.span(),
            "expected contents, identity, or contract(\"...\")",
        ));
    }
    let ident: Ident = input.parse()?;
    match ident.to_string().as_str() {
        "contents" => Ok(PreserveRef::Contents),
        "identity" => Ok(PreserveRef::Identity),
        "contract" => {
            let inner;
            parenthesized!(inner in input);
            let text: LitStr = inner.parse()?;
            expect_exhausted(&inner, "contract")?;
            Ok(PreserveRef::Contract(text.value()))
        }
        other => Err(syn::Error::new(
            ident.span(),
            format!(
                "unknown preserved property `{other}`; expected contents, identity, or contract(\"...\")"
            ),
        )),
    }
}

fn parse_mechanism(input: ParseStream<'_>) -> syn::Result<&'static str> {
    let ident: Ident = input.parse()?;
    let name = ident.to_string();
    MECHANISMS
        .iter()
        .find(|(mech, _)| *mech == name)
        .map(|(_, variant)| *variant)
        .ok_or_else(|| {
            syn::Error::new(
                ident.span(),
                format!(
                    "unknown transition mechanism `{name}`; expected one of {}",
                    mech_names().join(", ")
                ),
            )
        })
}

fn lookup(table: &'static [(&'static str, &'static str)], ident: &Ident) -> Option<&'static str> {
    table
        .iter()
        .find(|(name, _)| ident == name)
        .map(|(_, constant)| *constant)
}

/// Only the canonical dash-form names are shown in diagnostics.
fn builtin_keys(table: &'static [(&'static str, &'static str)]) -> Vec<&'static str> {
    table
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !name.contains('_'))
        .collect()
}

fn unknown_term_error(ident: &Ident, kind: &str, keys: Vec<&str>) -> syn::Error {
    syn::Error::new(
        ident.span(),
        format!(
            "unknown {kind} `{}`; expected one of {}, or custom(\"...\") for an open-set extension",
            ident,
            keys.join(", ")
        ),
    )
}
