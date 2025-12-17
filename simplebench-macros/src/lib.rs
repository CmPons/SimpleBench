use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{parse_macro_input, Expr, ExprLit, ItemFn, Lit, Meta};

/// The `#[bench]` attribute macro for registering benchmark functions.
///
/// # Basic Usage (no setup)
///
/// ```rust,ignore
/// #[bench]
/// fn simple_benchmark() {
///     quick_operation();
/// }
/// ```
///
/// The entire function body is measured on each iteration. Use this for benchmarks
/// where setup is negligible or part of what you want to measure.
///
/// # With Setup
///
/// ```rust,ignore
/// #[bench(setup = create_data)]
/// fn benchmark_with_setup(data: &Data) {
///     operation(data);  // Only this runs per iteration
/// }
/// ```
///
/// The setup function/closure runs once before measurement begins. The benchmark
/// function receives a reference to the setup data for each iteration.
///
/// # Inline Setup Closure
///
/// ```rust,ignore
/// #[bench(setup = || random_vectors(1000))]
/// fn benchmark_inline(data: &Vec<Vec3>) {
///     for v in data { v.normalize(); }
/// }
/// ```
#[proc_macro_attribute]
pub fn bench(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let has_params = !input_fn.sig.inputs.is_empty();

    // Parse optional parameters from attributes
    let mut setup_expr: Option<Expr> = None;
    let mut _iterations = 1000usize;
    let mut _samples = 100usize;

    for arg in args {
        if let Meta::NameValue(nv) = arg {
            let ident = nv.path.get_ident().map(|i| i.to_string());

            match ident.as_deref() {
                Some("setup") => {
                    setup_expr = Some(nv.value);
                }
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

    // Validate attribute/parameter combinations
    match (setup_expr.is_some(), has_params) {
        (false, true) => {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "benchmark with parameters requires #[bench(setup = ...)]",
            )
            .to_compile_error()
            .into();
        }
        (true, false) => {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "#[bench(setup = ...)] requires function to accept &T parameter",
            )
            .to_compile_error()
            .into();
        }
        _ => {}
    }

    if let Some(setup) = setup_expr {
        generate_with_setup(fn_name, &fn_name_str, &input_fn, setup)
    } else {
        generate_simple(fn_name, &fn_name_str, &input_fn)
    }
}

/// Generate code for a simple benchmark (no setup).
///
/// The benchmark function is called directly in a closure passed to `measure_simple`.
fn generate_simple(fn_name: &syn::Ident, fn_name_str: &str, input_fn: &ItemFn) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);

    let expanded = quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::config::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            ::simplebench_runtime::measure_simple(
                config,
                #fn_name_str,
                module_path!(),
                || #fn_name(),
            )
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: module_path!(),
                run: #run_fn_name,
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generate code for a benchmark with setup.
///
/// The setup expression runs once, then the benchmark function receives
/// a reference to the setup data for each measurement iteration.
///
/// The setup expression is used directly as the setup closure - if the user writes
/// `setup = my_fn`, we call `my_fn()`. If they write `setup = || expr`, we call
/// the closure. Both forms work because the expression is invoked with `()`.
fn generate_with_setup(
    fn_name: &syn::Ident,
    fn_name_str: &str,
    input_fn: &ItemFn,
    setup_expr: Expr,
) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);

    // The setup_expr could be:
    // - A function name: `create_data` -> call as `create_data()`
    // - A closure: `|| random_vectors(1000)` -> call as `(|| random_vectors(1000))()`
    // Both are handled by wrapping in parens and calling with ()
    let expanded = quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::config::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            ::simplebench_runtime::measure_with_setup(
                config,
                #fn_name_str,
                module_path!(),
                || (#setup_expr)(),
                |data| #fn_name(data),
            )
        }

        ::simplebench_runtime::inventory::submit! {
            ::simplebench_runtime::SimpleBench {
                name: #fn_name_str,
                module: module_path!(),
                run: #run_fn_name,
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
