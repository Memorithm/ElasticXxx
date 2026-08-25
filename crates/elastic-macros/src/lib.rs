//! Procedural macros for ElasticXxx.
//!
//! [`ElasticResource`] lowers a declarative attribute into exactly the same
//! typed semantic structures a programmer could build by hand with
//! `elastic_core::resource::ResourceSpec`. The macro contains **no**
//! independent semantics: every fragment maps one-to-one onto a builder call,
//! and all validation remains in the typed core (`ResourceSpecBuilder::build`).
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
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parenthesized, parse_macro_input, Attribute, Data, DeriveInput, Ident, LitStr, Token};

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

    let mut class_ref: Option<ClassRef> = None;
    let mut id: Option<String> = None;
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut combined_error: Option<syn::Error> = None;

    for attr in &elastic_attrs {
        let entries = attr.parse_args_with(Punctuated::<Entry, Token![,]>::parse_terminated)?;
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
    }

    if let Some(error) = combined_error {
        return Err(error);
    }

    let class_ref = class_ref.ok_or_else(|| {
        syn::Error::new(
            ident.span(),
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
            ident.span(),
            "missing mandatory elasticity: declare at least one allow(...) dimension",
        ));
    }

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
