use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::Token;
use syn::{parse_macro_input, Expr, ItemFn, Meta};

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
/// The entire function body is measured on each sample. Use this for benchmarks
/// where setup is negligible or part of what you want to measure.
///
/// # With Setup (runs once)
///
/// ```rust,ignore
/// #[bench(setup = create_data)]
/// fn benchmark_with_setup(data: &Data) {
///     operation(data);  // Only this runs per sample
/// }
/// ```
///
/// The setup function/closure runs once before measurement begins. The benchmark
/// function receives a reference to the setup data for each sample.
///
/// # With Setup Each (runs before every sample)
///
/// ```rust,ignore
/// // Owning: benchmark takes ownership (for operations that consume/mutate data)
/// #[bench(setup_each = || vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3])]
/// fn bench_sort(mut data: Vec<i32>) {
///     data.sort();
/// }
///
/// // Borrowing: benchmark takes reference (for fresh read-only data each sample)
/// #[bench(setup_each = || random_vectors(1000))]
/// fn bench_normalize(vectors: &Vec<Vec3>) {
///     for v in vectors { v.normalize(); }
/// }
/// ```
///
/// The setup expression runs before every sample. The benchmark function can take
/// either `T` (ownership) or `&T` (reference) depending on whether it consumes the data.
#[proc_macro_attribute]
pub fn bench(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let has_params = !input_fn.sig.inputs.is_empty();

    // Parse optional parameters from attributes
    let mut setup_expr: Option<Expr> = None;
    let mut setup_each_expr: Option<Expr> = None;

    for arg in args {
        if let Meta::NameValue(nv) = arg {
            let ident = nv.path.get_ident().map(|i| i.to_string());

            match ident.as_deref() {
                Some("setup") => {
                    setup_expr = Some(nv.value);
                }
                Some("setup_each") => {
                    setup_each_expr = Some(nv.value);
                }
                _ => {}
            }
        }
    }

    // Validate: cannot use both setup and setup_each
    if setup_expr.is_some() && setup_each_expr.is_some() {
        return syn::Error::new_spanned(
            &input_fn.sig,
            "cannot use both `setup` and `setup_each` - choose one",
        )
        .to_compile_error()
        .into();
    }

    // Validate attribute/parameter combinations
    if setup_each_expr.is_some() {
        // setup_each requires a parameter
        if !has_params {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "#[bench(setup_each = ...)] requires function to accept T or &T parameter",
            )
            .to_compile_error()
            .into();
        }
        return generate_with_setup_each(fn_name, &fn_name_str, &input_fn, setup_each_expr.unwrap());
    }

    if let Some(setup) = setup_expr {
        // setup (runs once) requires &T parameter
        if !has_params {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "#[bench(setup = ...)] requires function to accept &T parameter",
            )
            .to_compile_error()
            .into();
        }
        if !is_reference_param(&input_fn) {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "#[bench(setup = ...)] requires &T parameter; for T (ownership), use setup_each",
            )
            .to_compile_error()
            .into();
        }
        generate_with_setup(fn_name, &fn_name_str, &input_fn, setup)
    } else {
        // No setup - benchmark must not have parameters
        if has_params {
            return syn::Error::new_spanned(
                &input_fn.sig,
                "benchmark with parameters requires #[bench(setup = ...)] or #[bench(setup_each = ...)]",
            )
            .to_compile_error()
            .into();
        }
        generate_simple(fn_name, &fn_name_str, &input_fn)
    }
}

/// Check if the first parameter of the function is a reference type
fn is_reference_param(input_fn: &ItemFn) -> bool {
    if let Some(first_param) = input_fn.sig.inputs.first() {
        if let syn::FnArg::Typed(pat_type) = first_param {
            if let syn::Type::Reference(_) = &*pat_type.ty {
                return true;
            }
        }
    }
    false
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

/// Generate code for a benchmark with setup_each (runs before every sample).
///
/// Detects whether the benchmark takes `T` (ownership) or `&T` (reference)
/// and generates the appropriate measurement function call.
fn generate_with_setup_each(
    fn_name: &syn::Ident,
    fn_name_str: &str,
    input_fn: &ItemFn,
    setup_expr: Expr,
) -> TokenStream {
    let run_fn_name = format_ident!("__simplebench_run_{}", fn_name);
    let is_ref = is_reference_param(input_fn);

    let measure_call = if is_ref {
        // Benchmark takes &T - use borrowing version
        quote! {
            ::simplebench_runtime::measure_with_setup_each_ref(
                config,
                #fn_name_str,
                module_path!(),
                || (#setup_expr)(),
                |data| #fn_name(data),
            )
        }
    } else {
        // Benchmark takes T - use owning version
        quote! {
            ::simplebench_runtime::measure_with_setup_each(
                config,
                #fn_name_str,
                module_path!(),
                || (#setup_expr)(),
                |data| #fn_name(data),
            )
        }
    };

    let expanded = quote! {
        #input_fn

        fn #run_fn_name(
            config: &::simplebench_runtime::config::BenchmarkConfig
        ) -> ::simplebench_runtime::BenchResult {
            #measure_call
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
}
