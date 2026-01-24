//! This crate provides the derive macro for the `nami` crate.
//! It includes the `Project` derive macro and the `s!` procedural macro.

use proc_macro::TokenStream;
use quote::quote;
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

/// Function-like procedural macro for creating formatted string signals with automatic variable capture.
///
/// This macro automatically detects named variables in format strings and captures them from scope.
///
/// # Examples
///
/// ```rust
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
#[allow(clippy::similar_names)] // Allow arg1, arg2, etc.
pub fn s(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SInput);
    let format_str = input.format_str;
    let format_value = format_str.value();

    // Check for format string issues
    let (has_positional, has_named, positional_count, named_vars) =
        analyze_format_string(&format_value);

    // Separate named and positional arguments
    let mut positional_args: Vec<&Expr> = Vec::new();
    let mut named_args: Vec<(&syn::Ident, &Expr)> = Vec::new();

    for arg in &input.args {
        match arg {
            SArg::Positional(expr) => positional_args.push(expr),
            SArg::Named { name, value } => named_args.push((name, value)),
        }
    }

    // Check for mixed named and positional arguments
    if !positional_args.is_empty() && !named_args.is_empty() {
        return syn::Error::new_spanned(
            &format_str,
            "Cannot mix positional and named arguments. Use either all positional or all named.",
        )
        .to_compile_error()
        .into();
    }

    // Handle named arguments (like println! style: `s!("{c}", c = value)`)
    if !named_args.is_empty() {
        // Validate that format string has named placeholders
        if has_positional {
            return syn::Error::new_spanned(
                &format_str,
                "Format string has positional placeholders {{}} but named arguments were provided. \
                Use named placeholders like {{name}} with named arguments.",
            )
            .to_compile_error()
            .into();
        }

        // Check that all named placeholders have corresponding arguments
        let arg_names: Vec<String> = named_args.iter().map(|(n, _)| n.to_string()).collect();
        for var in &named_vars {
            if !arg_names.contains(var) {
                return syn::Error::new_spanned(
                    &format_str,
                    format!(
                        "Named placeholder {{{var}}} in format string has no corresponding argument. \
                        Add `{var} = <expr>` to the arguments."
                    ),
                )
                .to_compile_error()
                .into();
            }
        }

        // Check for unused arguments
        for (name, _) in &named_args {
            let name_str = name.to_string();
            if !named_vars.contains(&name_str) {
                return syn::Error::new_spanned(
                    name,
                    format!("Named argument `{name_str}` is not used in format string"),
                )
                .to_compile_error()
                .into();
            }
        }

        // Generate code for named arguments with automatic cloning
        let names: Vec<&syn::Ident> = named_args.iter().map(|(n, _)| *n).collect();
        let exprs: Vec<&Expr> = named_args.iter().map(|(_, e)| *e).collect();
        return handle_s_named_args(&format_str, &names, &exprs);
    }

    // Handle positional arguments
    if !positional_args.is_empty() {
        // Check for named placeholders with positional arguments
        if has_named {
            return syn::Error::new_spanned(
                &format_str,
                format!(
                    "Format string contains named placeholders like {{{}}} but positional arguments were provided. \
                    Either use positional placeholders like {{}} or use named arguments like `{} = <expr>`.",
                    named_vars.first().unwrap_or(&String::new()),
                    named_vars.first().unwrap_or(&String::new())
                )
            )
            .to_compile_error()
            .into();
        }

        // Check argument count matches placeholders
        if positional_count != positional_args.len() {
            return syn::Error::new_spanned(
                &format_str,
                format!(
                    "Format string has {} positional placeholder(s) but {} arguments were provided",
                    positional_count,
                    positional_args.len()
                ),
            )
            .to_compile_error()
            .into();
        }
        return handle_s_args(&format_str, &positional_args);
    }

    // No explicit arguments - check for automatic capture

    // Check for mixed placeholders when no explicit arguments
    if has_positional && has_named {
        return syn::Error::new_spanned(
            &format_str,
            "Format string mixes positional {{}} and named {{var}} placeholders. \
            Use either all positional with explicit arguments, or all named for automatic capture.",
        )
        .to_compile_error()
        .into();
    }

    // If has positional placeholders but no arguments provided
    if has_positional {
        return syn::Error::new_spanned(
            &format_str,
            format!(
                "Format string has {positional_count} positional placeholder(s) {{}} but no arguments provided. \
                Either provide arguments or use named placeholders like {{variable}} for automatic capture."
            )
        )
        .to_compile_error()
        .into();
    }

    // Parse format string to extract variable names for automatic capture
    let var_names = named_vars;

    // If no variables found, return constant
    if var_names.is_empty() {
        return quote! {
            {
                use ::nami::constant;
                constant(::nami::__alloc::format!(#format_str))
            }
        }
        .into();
    }

    // Generate code for named variable capture
    let var_idents: Vec<syn::Ident> = var_names
        .iter()
        .map(|name| syn::Ident::new(name, format_str.span()))
        .collect();

    handle_s_named_vars(&format_str, &var_idents)
}

#[allow(clippy::similar_names)]
fn handle_s_args(format_str: &LitStr, args: &[&Expr]) -> TokenStream {
    match args.len() {
        1 => {
            let arg = &args[0];
            (quote! {
                {
                    use ::nami::SignalExt;
                    (#arg).map(|arg| ::nami::__alloc::format!(#format_str, arg))
                }
            })
            .into()
        }
        2 => {
            let arg1 = &args[0];
            let arg2 = &args[1];
            (quote! {
                {
                    use nami::{SignalExt, zip::zip};
                    zip(#arg1.clone(), #arg2.clone()).map(|(arg1, arg2)| {
                        ::nami::__alloc::format!(#format_str, arg1, arg2)
                    })
                }
            })
            .into()
        }
        3 => {
            let arg1 = &args[0];
            let arg2 = &args[1];
            let arg3 = &args[2];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(zip(#arg1.clone(), #arg2.clone()), #arg3.clone()).map(
                        |((arg1, arg2), arg3)| ::nami::__alloc::format!(#format_str, arg1, arg2, arg3)
                    )
                }
            })
            .into()
        }
        4 => {
            let arg1 = &args[0];
            let arg2 = &args[1];
            let arg3 = &args[2];
            let arg4 = &args[3];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(
                        zip(#arg1.clone(), #arg2.clone()),
                        zip(#arg3.clone(), #arg4.clone())
                    ).map(
                        |((arg1, arg2), (arg3, arg4))| ::nami::__alloc::format!(#format_str, arg1, arg2, arg3, arg4)
                    )
                }
            }).into()
        }
        _ => syn::Error::new_spanned(format_str, "Too many arguments, maximum 4 supported")
            .to_compile_error()
            .into(),
    }
}

/// Handle named arguments like `s!("{c}", c = expr)` with automatic cloning
///
/// Uses `ToOwned::to_owned()` to properly handle both owned values and references:
/// - If expr is `&Binding<T>`, to_owned() returns `Binding<T>`
/// - If expr is `Binding<T>`, to_owned() also returns `Binding<T>`
#[allow(clippy::similar_names)]
fn handle_s_named_args(
    format_str: &LitStr,
    names: &[&syn::Ident],
    exprs: &[&Expr],
) -> TokenStream {
    match names.len() {
        1 => {
            let name = names[0];
            let expr = exprs[0];
            (quote! {
                {
                    use ::nami::SignalExt;
                    ::nami::__alloc::borrow::ToOwned::to_owned(#expr).map(|#name| {
                        ::::nami::__alloc::format!(#format_str)
                    })
                }
            })
            .into()
        }
        2 => {
            let name1 = names[0];
            let name2 = names[1];
            let expr1 = exprs[0];
            let expr2 = exprs[1];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(::nami::__alloc::borrow::ToOwned::to_owned(#expr1), ::nami::__alloc::borrow::ToOwned::to_owned(#expr2)).map(|(#name1, #name2)| {
                        ::::nami::__alloc::format!(#format_str)
                    })
                }
            })
            .into()
        }
        3 => {
            let name1 = names[0];
            let name2 = names[1];
            let name3 = names[2];
            let expr1 = exprs[0];
            let expr2 = exprs[1];
            let expr3 = exprs[2];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(
                        zip(::nami::__alloc::borrow::ToOwned::to_owned(#expr1), ::nami::__alloc::borrow::ToOwned::to_owned(#expr2)),
                        ::nami::__alloc::borrow::ToOwned::to_owned(#expr3)
                    ).map(
                        |((#name1, #name2), #name3)| {
                            ::::nami::__alloc::format!(#format_str)
                        }
                    )
                }
            })
            .into()
        }
        4 => {
            let name1 = names[0];
            let name2 = names[1];
            let name3 = names[2];
            let name4 = names[3];
            let expr1 = exprs[0];
            let expr2 = exprs[1];
            let expr3 = exprs[2];
            let expr4 = exprs[3];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(
                        zip(::nami::__alloc::borrow::ToOwned::to_owned(#expr1), ::nami::__alloc::borrow::ToOwned::to_owned(#expr2)),
                        zip(::nami::__alloc::borrow::ToOwned::to_owned(#expr3), ::nami::__alloc::borrow::ToOwned::to_owned(#expr4))
                    ).map(
                        |((#name1, #name2), (#name3, #name4))| {
                            ::::nami::__alloc::format!(#format_str)
                        }
                    )
                }
            })
            .into()
        }
        _ => syn::Error::new_spanned(format_str, "Too many named arguments, maximum 4 supported")
            .to_compile_error()
            .into(),
    }
}

#[allow(clippy::similar_names)]
fn handle_s_named_vars(format_str: &LitStr, var_idents: &[syn::Ident]) -> TokenStream {
    match var_idents.len() {
        1 => {
            let var = &var_idents[0];
            (quote! {
                {
                    use ::nami::SignalExt;
                    (#var).map(|#var| {
                        ::nami::__alloc::format!(#format_str)
                    })
                }
            })
            .into()
        }
        2 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(#var1.clone(), #var2.clone()).map(|(#var1, #var2)| {
                        ::nami::__alloc::format!(#format_str)
                    })
                }
            })
            .into()
        }
        3 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            let var3 = &var_idents[2];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(zip(#var1.clone(), #var2.clone()), #var3.clone()).map(
                        |((#var1, #var2), #var3)| {
                            ::::nami::__alloc::format!(#format_str)
                        }
                    )
                }
            })
            .into()
        }
        4 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            let var3 = &var_idents[2];
            let var4 = &var_idents[3];
            (quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    zip(
                        zip(#var1.clone(), #var2.clone()),
                        zip(#var3.clone(), #var4.clone())
                    ).map(
                        |((#var1, #var2), (#var3, #var4))| {
                            ::::nami::__alloc::format!(#format_str)
                        }
                    )
                }
            })
            .into()
        }
        _ => syn::Error::new_spanned(format_str, "Too many named variables, maximum 4 supported")
            .to_compile_error()
            .into(),
    }
}

/// Analyze a format string to detect placeholder types and extract variable names
fn analyze_format_string(format_str: &str) -> (bool, bool, usize, Vec<String>) {
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

    (has_positional, has_named, positional_count, named_vars)
}
