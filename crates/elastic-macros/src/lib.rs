//! Procedural macro entrypoints for ElasticXxx.
//!
//! Parsing and expansion live in `memorithm-elastic-language-syntax` so the
//! compiler macro and `cargo elastic expand` use exactly the same implementation.

use proc_macro::TokenStream;

#[proc_macro_derive(ElasticResource, attributes(elastic))]
pub fn derive_elastic_resource(input: TokenStream) -> TokenStream {
    match elastic_language_syntax::expand_derive_tokens(input.into()) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[proc_macro]
pub fn elastic(input: TokenStream) -> TokenStream {
    match elastic_language_syntax::expand_elastic_tokens(input.into()) {
        Ok(expanded) => expanded.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
