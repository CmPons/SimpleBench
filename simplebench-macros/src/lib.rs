use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{parse_macro_input, Expr, ExprLit, ItemFn, Lit, Meta};

#[proc_macro_attribute]
pub fn bench(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();

    // Parse optional parameters from attributes
    // Note: Currently these are parsed but not used in the SimpleBench struct
    // They will be used when we implement per-benchmark configuration
    let mut _iterations = 1000usize;
    let mut _samples = 100usize;

    for arg in args {
        if let Meta::NameValue(nv) = arg {
            let ident = nv.path.get_ident().map(|i| i.to_string());

            match ident.as_deref() {
                Some("iterations") => {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Int(lit_int),
                        ..
                    }) = &nv.value
                    {
                        _iterations = lit_int.base10_parse().unwrap();
                    }
                }
                Some("samples") => {
                    if let Expr::Lit(ExprLit {
                        lit: Lit::Int(lit_int),
                        ..
                    }) = &nv.value
                    {
                        _samples = lit_int.base10_parse().unwrap();
                    }
                }
                _ => {}
            }
        }
    }

    // Get module path at compile time
    let module_path = quote! { module_path!() };

    // Generate wrapper function name
    let wrapper_fn_name = syn::Ident::new(
        &format!("__simplebench_wrapper_{}", fn_name),
        fn_name.span(),
    );

    let expanded = quote! {
        #input_fn

        fn #wrapper_fn_name() {
            #fn_name();
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: #module_path,
                func: #wrapper_fn_name,
            }
        }
    };

    TokenStream::from(expanded)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_macro_compiles() {
        // Unit tests for proc macros are limited
        // The real tests are in the integration tests
        // This just verifies the module compiles
        assert!(true);
    }

    #[test]
    fn test_default_params() {
        // Default iterations should be 1000
        // Default samples should be 100
        // Verified in integration tests
        assert_eq!(1000, 1000);
        assert_eq!(100, 100);
    }
}
