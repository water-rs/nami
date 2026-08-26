//! This crate provides the derive macro for the `nami` crate.
//! It includes the `Project` derive macro and the `s!` procedural macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::Parse, parse_macro_input, punctuated::Punctuated, Data, DeriveInput, Expr, Fields,
    LitStr, Token, Type,
};

/// Derive macro for implementing the `Project` trait on structs.
///
/// This macro automatically generates a `Project` implementation that allows
/// decomposing a struct binding into separate bindings for each field.
///
/// # Examples
///
/// ```rust,ignore
/// use nami::{Binding, binding};
/// use nami_derive::Project;
///
/// #[derive(Project,Clone,Debug)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person_binding: Binding<Person> = binding(Person {
///     name: "Alice".to_string(),
///     age: 30,
/// });
///
/// let mut projected = person_binding.project();
/// projected.name.set_from("Bob");
/// projected.age.set(25);
///
/// let person = person_binding.get();
/// assert_eq!(person.name, "Bob");
/// assert_eq!(person.age, 25);
/// ```
#[proc_macro_derive(Project)]
pub fn derive_project(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => derive_project_struct(&input, fields_named),
            Fields::Unnamed(fields_unnamed) => derive_project_tuple_struct(&input, fields_unnamed),
            Fields::Unit => derive_project_unit_struct(&input),
        },
        Data::Enum(_) => {
            syn::Error::new_spanned(input, "Project derive macro does not support enums")
                .to_compile_error()
                .into()
        }
        Data::Union(_) => {
            syn::Error::new_spanned(input, "Project derive macro does not support unions")
                .to_compile_error()
                .into()
        }
    }
}

fn derive_project_struct(input: &DeriveInput, fields: &syn::FieldsNamed) -> TokenStream {
    let struct_name = &input.ident;
    let (_impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Create the projected struct type
    let projected_struct_name =
        syn::Ident::new(&format!("{struct_name}Projected"), struct_name.span());

    // Generate fields for the projected struct
    let projected_fields = fields.named.iter().map(|field| {
        let field_name = &field.ident;
        let field_type = &field.ty;
        quote! {
            pub #field_name: ::nami::Binding<#field_type>
        }
    });

    // Generate the projection logic
    let field_projections = fields.named.iter().map(|field| {
        let field_name = &field.ident;
        quote! {
            #field_name: {
                let source = source.clone();
                ::nami::Binding::mapping(
                    &source,
                    |value| value.#field_name.clone(),
                    move |binding, value| {
                        binding.with_mut(|b| {
                            b.#field_name = value;
                        });
                    },
                )
            }
        }
    });

    // Add lifetime bounds to generic parameters
    let mut generics_with_static = input.generics.clone();
    for param in &mut generics_with_static.params {
        if let syn::GenericParam::Type(type_param) = param {
            type_param.bounds.push(syn::parse_quote!('static));
        }
    }
    let (impl_generics_with_static, _, _) = generics_with_static.split_for_impl();

    let expanded = quote! {
        /// Projected version of #struct_name with each field wrapped in a Binding.
        #[derive(Debug)]
        pub struct #projected_struct_name #ty_generics #where_clause {
            #(#projected_fields,)*
        }

        impl #impl_generics_with_static ::nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = #projected_struct_name #ty_generics;

            fn project(source: &::nami::Binding<Self>) -> Self::Projected {
                #projected_struct_name {
                    #(#field_projections,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn derive_project_tuple_struct(input: &DeriveInput, fields: &syn::FieldsUnnamed) -> TokenStream {
    let struct_name = &input.ident;
    let (_impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Generate tuple type for projection
    let field_types: Vec<&Type> = fields.unnamed.iter().map(|field| &field.ty).collect();
    let projected_tuple = if field_types.len() == 1 {
        quote! { (::nami::Binding<#(#field_types)*>,) }
    } else {
        quote! { (#(::nami::Binding<#field_types>),*) }
    };

    // Generate field projections using index access
    let field_projections = fields.unnamed.iter().enumerate().map(|(index, _)| {
        let idx = syn::Index::from(index);
        quote! {
            {
                let source = source.clone();
                ::nami::Binding::mapping(
                    &source,
                    |value| value.#idx.clone(),
                    move |binding, value| {
                        binding.with_mut(|b| {
                            b.#idx = value;
                        });
                    },
                )
            }
        }
    });

    // Add lifetime bounds to generic parameters
    let mut generics_with_static = input.generics.clone();
    for param in &mut generics_with_static.params {
        if let syn::GenericParam::Type(type_param) = param {
            type_param.bounds.push(syn::parse_quote!('static));
        }
    }
    let (impl_generics_with_static, _, _) = generics_with_static.split_for_impl();

    let projection_tuple = if field_projections.len() == 1 {
        quote! { (#(#field_projections)*,) }
    } else {
        quote! { (#(#field_projections),*) }
    };

    let expanded = quote! {
        impl #impl_generics_with_static ::nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = #projected_tuple;

            fn project(source: &::nami::Binding<Self>) -> Self::Projected {
                #projection_tuple
            }
        }
    };

    TokenStream::from(expanded)
}

fn derive_project_unit_struct(input: &DeriveInput) -> TokenStream {
    let struct_name = &input.ident;
    let (_impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Add lifetime bounds to generic parameters
    let mut generics_with_static = input.generics.clone();
    for param in &mut generics_with_static.params {
        if let syn::GenericParam::Type(type_param) = param {
            type_param.bounds.push(syn::parse_quote!('static));
        }
    }
    let (impl_generics_with_static, _, _) = generics_with_static.split_for_impl();

    let expanded = quote! {
        impl #impl_generics_with_static ::nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = ();

            fn project(_source: &::nami::Binding<Self>) -> Self::Projected {
                ()
            }
        }
    };

    TokenStream::from(expanded)
}

/// A single argument to the `s!` macro - either positional or named
enum SArg {
    /// Positional argument: just an expression
    Positional(Expr),
    /// Named argument: `name = expr`
    Named { name: syn::Ident, value: Expr },
}

impl Parse for SArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Try to parse as named argument: `ident = expr`
        if input.peek(syn::Ident) && input.peek2(Token![=]) {
            let name: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: Expr = input.parse()?;
            Ok(Self::Named { name, value })
        } else {
            // Parse as positional expression
            let expr: Expr = input.parse()?;
            Ok(Self::Positional(expr))
        }
    }
}

/// Input structure for the `s!` macro
struct SInput {
    format_str: LitStr,
    args: Punctuated<SArg, Token![,]>,
}

impl Parse for SInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let format_str: LitStr = input.parse()?;
        let args = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            Punctuated::parse_terminated(input)?
        } else {
            Punctuated::new()
        };
        Ok(Self { format_str, args })
    }
}

/// Result of analyzing a format string's placeholders.
struct FormatAnalysis {
    has_positional: bool,
    has_named: bool,
    positional_count: usize,
    named_vars: Vec<String>,
}

/// A single entry in the zip tree: the expression to zip and the identifier to bind.
struct ZipEntry {
    /// The expression to pass into the zip tree (e.g. `expr.clone()` or `ToOwned::to_owned(expr)`).
    zip_expr: TokenStream2,
    /// The identifier bound in the closure pattern.
    name: syn::Ident,
}

/// Function-like procedural macro for creating formatted string signals with automatic variable capture.
///
/// This macro automatically detects named variables in format strings and captures them from scope.
///
/// # Examples
///
/// ```rust,ignore
/// use nami::*;
///
/// let name = constant("Alice");
/// let age = constant(25);
///
/// // Automatic variable capture from format string
/// let msg = s!("Hello {name}, you are {age} years old");
///
/// // Positional arguments still work
/// let msg2 = s!("Hello {}, you are {}", name, age);
///
/// // Named arguments with automatic cloning (like println!)
/// let msg3 = s!("Hello {n}, you are {a} years old", n = name, a = age);
/// ```
#[proc_macro]
pub fn s(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SInput);
    match expand_s(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Main expansion logic for the `s!` macro, returning a `syn::Result` for clean error handling.
fn expand_s(input: SInput) -> syn::Result<TokenStream2> {
    let format_str = input.format_str;
    let analysis = analyze_format_string(&format_str.value());

    // Separate named and positional arguments
    let mut positional_args: Vec<&Expr> = Vec::new();
    let mut named_args: Vec<(&syn::Ident, &Expr)> = Vec::new();

    for arg in &input.args {
        match arg {
            SArg::Positional(expr) => positional_args.push(expr),
            SArg::Named { name, value } => named_args.push((name, value)),
        }
    }

    validate_s_input(&format_str, &analysis, &positional_args, &named_args)?;

    // Determine mode and build entries
    if !named_args.is_empty() {
        // Named argument mode: `s!("{c}", c = value)`
        let entries: Vec<ZipEntry> = named_args
            .iter()
            .map(|(name, expr)| ZipEntry {
                zip_expr: quote! { ::nami::__alloc::borrow::ToOwned::to_owned(#expr) },
                name: (*name).clone(),
            })
            .collect();
        Ok(generate_s_code(&format_str, &entries, &[]))
    } else if !positional_args.is_empty() {
        // Positional argument mode: `s!("{} {}", a, b)`
        let entries: Vec<ZipEntry> = positional_args
            .iter()
            .enumerate()
            .map(|(i, expr)| {
                let name = format_ident!("__arg{i}");
                ZipEntry {
                    zip_expr: quote! { (#expr).clone() },
                    name,
                }
            })
            .collect();
        let format_extra: Vec<syn::Ident> = entries.iter().map(|e| e.name.clone()).collect();
        Ok(generate_s_code(&format_str, &entries, &format_extra))
    } else if analysis.named_vars.is_empty() {
        // Constant string, no placeholders
        Ok(quote! {
            {
                use ::nami::constant;
                constant(::nami::__alloc::format!(#format_str))
            }
        })
    } else {
        // Auto-capture mode: `s!("Hello {name}")`
        let var_idents: Vec<syn::Ident> = analysis
            .named_vars
            .iter()
            .map(|name| syn::Ident::new(name, format_str.span()))
            .collect();
        let entries: Vec<ZipEntry> = var_idents
            .iter()
            .map(|ident| ZipEntry {
                zip_expr: quote! { #ident.clone() },
                name: ident.clone(),
            })
            .collect();
        Ok(generate_s_code(&format_str, &entries, &[]))
    }
}

/// Validate all s! macro inputs, returning compile errors for misuse.
fn validate_s_input(
    format_str: &LitStr,
    analysis: &FormatAnalysis,
    positional_args: &[&Expr],
    named_args: &[(&syn::Ident, &Expr)],
) -> syn::Result<()> {
    // Mixed positional and named arguments
    if !positional_args.is_empty() && !named_args.is_empty() {
        return Err(syn::Error::new_spanned(
            format_str,
            "Cannot mix positional and named arguments. Use either all positional or all named.",
        ));
    }

    if !named_args.is_empty() {
        // Named args provided but format string has positional placeholders
        if analysis.has_positional {
            return Err(syn::Error::new_spanned(
                format_str,
                "Format string has positional placeholders {{}} but named arguments were provided. \
                Use named placeholders like {{name}} with named arguments.",
            ));
        }

        // Check all named placeholders have corresponding arguments
        let arg_names: Vec<String> = named_args.iter().map(|(n, _)| n.to_string()).collect();
        for var in &analysis.named_vars {
            if !arg_names.contains(var) {
                return Err(syn::Error::new_spanned(
                    format_str,
                    format!(
                        "Named placeholder {{{var}}} in format string has no corresponding argument. \
                        Add `{var} = <expr>` to the arguments."
                    ),
                ));
            }
        }

        // Check for unused arguments
        for (name, _) in named_args {
            let name_str = name.to_string();
            if !analysis.named_vars.contains(&name_str) {
                return Err(syn::Error::new_spanned(
                    name,
                    format!("Named argument `{name_str}` is not used in format string"),
                ));
            }
        }
    } else if !positional_args.is_empty() {
        // Positional args provided but format has named placeholders
        if analysis.has_named {
            return Err(syn::Error::new_spanned(
                format_str,
                format!(
                    "Format string contains named placeholders like {{{}}} but positional arguments were provided. \
                    Either use positional placeholders like {{}} or use named arguments like `{} = <expr>`.",
                    analysis.named_vars.first().unwrap_or(&String::new()),
                    analysis.named_vars.first().unwrap_or(&String::new())
                ),
            ));
        }

        // Check argument count matches placeholders
        if analysis.positional_count != positional_args.len() {
            return Err(syn::Error::new_spanned(
                format_str,
                format!(
                    "Format string has {} positional placeholder(s) but {} arguments were provided",
                    analysis.positional_count,
                    positional_args.len()
                ),
            ));
        }
    } else {
        // No explicit arguments
        if analysis.has_positional && analysis.has_named {
            return Err(syn::Error::new_spanned(
                format_str,
                "Format string mixes positional {{}} and named {{var}} placeholders. \
                Use either all positional with explicit arguments, or all named for automatic capture.",
            ));
        }

        if analysis.has_positional {
            return Err(syn::Error::new_spanned(
                format_str,
                format!(
                    "Format string has {} positional placeholder(s) {{}} but no arguments provided. \
                    Either provide arguments or use named placeholders like {{variable}} for automatic capture.",
                    analysis.positional_count
                ),
            ));
        }
    }

    Ok(())
}

/// Recursively build a balanced zip tree from a slice of expressions.
///
/// Splits left-heavy (left gets `ceil(n/2)` elements) to match the existing
/// shape for 3 args: `zip(zip(a, b), c)`.
fn build_zip_tree(exprs: &[&TokenStream2]) -> TokenStream2 {
    match exprs.len() {
        0 => unreachable!("build_zip_tree called with 0 expressions"),
        1 => {
            let expr = exprs[0];
            quote! { #expr }
        }
        _ => {
            let mid = exprs.len().div_ceil(2);
            let left = build_zip_tree(&exprs[..mid]);
            let right = build_zip_tree(&exprs[mid..]);
            quote! { ::nami::zip::zip(#left, #right) }
        }
    }
}

/// Recursively build the destructure pattern matching the shape of `build_zip_tree`.
fn build_destructure_pattern(names: &[&syn::Ident]) -> TokenStream2 {
    match names.len() {
        0 => unreachable!("build_destructure_pattern called with 0 names"),
        1 => {
            let name = names[0];
            quote! { #name }
        }
        _ => {
            let mid = names.len().div_ceil(2);
            let left = build_destructure_pattern(&names[..mid]);
            let right = build_destructure_pattern(&names[mid..]);
            quote! { (#left, #right) }
        }
    }
}

/// Unified code generation for all `s!` modes.
///
/// - 0 entries: should have been handled by caller (constant string).
/// - 1 entry: `(expr).map(|name| format!(...))` — no zip needed.
/// - N entries: `zip_tree.map(|pattern| format!(...))`.
///
/// `format_extra_args` are passed as extra arguments to `format!()` after the
/// format string (used only for positional mode where format placeholders are `{}`).
fn generate_s_code(
    format_str: &LitStr,
    entries: &[ZipEntry],
    format_extra_args: &[syn::Ident],
) -> TokenStream2 {
    assert!(!entries.is_empty(), "generate_s_code called with 0 entries");

    let names: Vec<&syn::Ident> = entries.iter().map(|e| &e.name).collect();

    // Build the format!() call — with or without extra positional args
    let format_call = if format_extra_args.is_empty() {
        quote! { ::nami::__alloc::format!(#format_str) }
    } else {
        quote! { ::nami::__alloc::format!(#format_str, #(#format_extra_args),*) }
    };

    if entries.len() == 1 {
        // Single entry: no zip, direct map
        let zip_expr = &entries[0].zip_expr;
        let name = &entries[0].name;
        quote! {
            {
                use ::nami::SignalExt;
                (#zip_expr).map(|#name| #format_call)
            }
        }
    } else {
        // Multiple entries: build zip tree + destructure pattern
        let zip_exprs: Vec<&TokenStream2> = entries.iter().map(|e| &e.zip_expr).collect();
        let name_refs: Vec<&syn::Ident> = names.clone();

        let zip_tree = build_zip_tree(&zip_exprs);
        let pattern = build_destructure_pattern(&name_refs);

        quote! {
            {
                use ::nami::{SignalExt, zip::zip};
                #zip_tree.map(|#pattern| #format_call)
            }
        }
    }
}

/// Analyze a format string to detect placeholder types and extract variable names.
fn analyze_format_string(format_str: &str) -> FormatAnalysis {
    let mut has_positional = false;
    let mut has_named = false;
    let mut positional_count = 0;
    let mut named_vars = Vec::new();
    let mut chars = format_str.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                // Skip escaped braces
                chars.next();
                continue;
            }

            let mut content = String::new();
            let mut has_content = false;

            while let Some(&next_char) = chars.peek() {
                if next_char == '}' {
                    chars.next(); // consume }
                    break;
                } else if next_char == ':' {
                    // Format specifier found, we've captured the name/position part
                    chars.next(); // consume :
                    while let Some(&spec_char) = chars.peek() {
                        if spec_char == '}' {
                            chars.next(); // consume }
                            break;
                        }
                        chars.next();
                    }
                    break;
                }
                content.push(chars.next().unwrap());
                has_content = true;
            }

            // Analyze the content
            if !has_content || content.is_empty() {
                // Empty {} is positional
                has_positional = true;
                positional_count += 1;
            } else if content.chars().all(|ch| ch.is_ascii_digit()) {
                // Numeric like {0} or {1} is positional
                has_positional = true;
                positional_count += 1;
            } else if content
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            {
                // Starts with letter or underscore, likely a variable name
                has_named = true;
                if !named_vars.contains(&content) {
                    named_vars.push(content);
                }
            } else {
                // Other cases treat as positional
                has_positional = true;
                positional_count += 1;
            }
        }
    }

    FormatAnalysis {
        has_positional,
        has_named,
        positional_count,
        named_vars,
    }
}
