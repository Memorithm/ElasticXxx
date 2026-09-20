#![no_main]

use elastic_language_syntax::{expand_derive_tokens, expand_elastic_tokens};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&mode, payload)) = data.split_first() else {
        return;
    };
    let Ok(source) = std::str::from_utf8(payload) else {
        return;
    };
    let Ok(tokens) = source.parse::<proc_macro2::TokenStream>() else {
        return;
    };

    if mode & 1 == 0 {
        let _ = expand_elastic_tokens(tokens);
    } else {
        let _ = expand_derive_tokens(tokens);
    }
});
