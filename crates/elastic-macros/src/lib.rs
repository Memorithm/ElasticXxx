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
use quote::quote;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    braced, parenthesized, parse_macro_input, Attribute, Data, DeriveInput, Ident, LitStr, Token,
    Visibility,
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
/// `EirDocumentBuilder`. Resource bodies accept exactly the same declaration
/// fragments as `#[derive(ElasticResource)]`, so the DSL owns no independent
/// resource, planning, or runtime semantics.
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
    syn::custom_keyword!(document);
    syn::custom_keyword!(resource);
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

struct ElasticDocumentDsl {
    visibility: Visibility,
    name: Ident,
    resources: Vec<ElasticDocumentResourceDsl>,
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
                    "elastic! single-resource form accepts exactly one resource; use `document NAME { resource ... }` for multiple resources",
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
            let mut names = std::collections::BTreeSet::new();
            while !content.is_empty() {
                content.parse::<kw::resource>().map_err(|_| {
                    syn::Error::new(
                        content.span(),
                        "elastic! document bodies accept only `resource NAME { ... }` declarations in v0.2",
                    )
                })?;
                let resource_name: Ident = content.parse()?;
                if resource_name == "document" {
                    return Err(syn::Error::new(
                        resource_name.span(),
                        "resource module name `document` is reserved for the generated document() function",
                    ));
                }
                if !names.insert(resource_name.to_string()) {
                    return Err(syn::Error::new(
                        resource_name.span(),
                        format!("duplicate resource module `{resource_name}` in elastic! document"),
                    ));
                }
                let resource_content;
                braced!(resource_content in content);
                let body: proc_macro2::TokenStream = resource_content.parse()?;
                resources.push(ElasticDocumentResourceDsl {
                    name: resource_name,
                    body,
                });
            }
            if resources.is_empty() {
                return Err(syn::Error::new(
                    name.span(),
                    "elastic! document must contain at least one resource",
                ));
            }
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
            }));
        }
        Err(syn::Error::new(
            input.span(),
            "expected `resource NAME { ... }` or `document NAME { resource ... }` after optional visibility",
        ))
    }
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

fn expand_document_module(
    input: ElasticDocumentDsl,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let ElasticDocumentDsl {
        visibility,
        name,
        resources,
    } = input;
    let resource_count = resources.len();
    let mut resource_modules = Vec::with_capacity(resource_count);
    let mut resource_names = Vec::with_capacity(resource_count);
    for resource in resources {
        let resource_name = resource.name.clone();
        resource_modules.push(expand_resource_module(ElasticResourceDsl {
            visibility: syn::parse_quote!(pub),
            name: resource.name,
            body: resource.body,
        })?);
        resource_names.push(resource_name);
    }

    Ok(quote! {
        #visibility mod #name {
            const _: () = {
                assert!(
                    #resource_count <= ::elastic::MAX_EIR_DOCUMENT_RESOURCES,
                    "elastic! document exceeds MAX_EIR_DOCUMENT_RESOURCES",
                );
            };

            #(#resource_modules)*

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
