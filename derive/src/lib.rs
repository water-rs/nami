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
/// ```rust
/// use nami::{Binding, binding};
/// use nami_derive::Project;
///
/// #[derive(Project)]
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
/// let projected = person_binding.project();
/// projected.name.set("Bob".to_string());
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
        syn::Ident::new(&format!("{}Projected", struct_name), struct_name.span());

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
                        binding.get_mut().#field_name = value;
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
                        binding.get_mut().#idx = value;
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

/// Input structure for the `s!` macro
struct SInput {
    format_str: LitStr,
    args: Punctuated<Expr, Token![,]>,
}

impl Parse for SInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let format_str: LitStr = input.parse()?;
        let mut args = Punctuated::new();

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            args = Punctuated::parse_terminated(input)?;
        }

        Ok(SInput { format_str, args })
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
/// ```
#[proc_macro]
pub fn s(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SInput);
    let format_str = input.format_str;
    let format_value = format_str.value();

    // Check for format string issues
    let (has_positional, has_named, positional_count, named_vars) =
        analyze_format_string(&format_value);

    // If there are explicit arguments, validate and use positional approach
    if !input.args.is_empty() {
        // Check for mixed usage errors
        if has_named {
            return syn::Error::new_spanned(
                &format_str,
                format!(
                    "Format string contains named arguments like {{{}}} but you provided positional arguments. \
                    Either use positional placeholders like {{}} or remove the explicit arguments to use automatic variable capture.",
                    named_vars.first().unwrap_or(&String::new())
                )
            )
            .to_compile_error()
            .into();
        }

        // Check argument count matches placeholders
        if positional_count != input.args.len() {
            return syn::Error::new_spanned(
                &format_str,
                format!(
                    "Format string has {} positional placeholders but {} arguments were provided",
                    positional_count,
                    input.args.len()
                ),
            )
            .to_compile_error()
            .into();
        }
        let args: Vec<_> = input.args.iter().collect();
        return match args.len() {
            1 => {
                let arg = &args[0];
                quote! {
                    {
                        use ::nami::SignalExt;
                        SignalExt::map(#arg.clone(), |arg| nami::__format!(#format_str, arg))
                    }
                }
                .into()
            }
            2 => {
                let arg1 = &args[0];
                let arg2 = &args[1];
                quote! {
                    {
                        use nami::{SignalExt, zip::zip};
                        SignalExt::map(zip(#arg1.clone(), #arg2.clone()), |(arg1, arg2)| {
                            nami::__format!(#format_str, arg1, arg2)
                        })
                    }
                }
                .into()
            }
            3 => {
                let arg1 = &args[0];
                let arg2 = &args[1];
                let arg3 = &args[2];
                quote! {
                    {
                        use ::nami::{SignalExt, zip::zip};
                        SignalExt::map(
                            zip(zip(#arg1.clone(), #arg2.clone()), #arg3.clone()),
                            |((arg1, arg2), arg3)| nami::__format!(#format_str, arg1, arg2, arg3)
                        )
                    }
                }
                .into()
            }
            4 => {
                let arg1 = &args[0];
                let arg2 = &args[1];
                let arg3 = &args[2];
                let arg4 = &args[3];
                quote! {
                    {
                        use ::nami::{SignalExt, zip::zip};
                        SignalExt::map(
                            zip(
                                zip(#arg1.clone(), #arg2.clone()),
                                zip(#arg3.clone(), #arg4.clone())
                            ),
                            |((arg1, arg2), (arg3, arg4))| nami::__format!(#format_str, arg1, arg2, arg3, arg4)
                        )
                    }
                }.into()
            }
            _ => syn::Error::new_spanned(format_str, "Too many arguments, maximum 4 supported")
                .to_compile_error()
                .into(),
        };
    }

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
    if has_positional && input.args.is_empty() {
        return syn::Error::new_spanned(
            &format_str,
            format!(
                "Format string has {} positional placeholder(s) {{}} but no arguments provided. \
                Either provide arguments or use named placeholders like {{variable}} for automatic capture.",
                positional_count
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
                constant(nami::__format!(#format_str))
            }
        }
        .into();
    }

    // Generate code for named variable capture
    let var_idents: Vec<syn::Ident> = var_names
        .iter()
        .map(|name| syn::Ident::new(name, format_str.span()))
        .collect();

    match var_names.len() {
        1 => {
            let var = &var_idents[0];
            quote! {
                {
                    use ::nami::SignalExt;
                    SignalExt::map(#var.clone(), |#var| {
                        nami::__format!(#format_str)
                    })
                }
            }
            .into()
        }
        2 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    SignalExt::map(zip(#var1.clone(), #var2.clone()), |(#var1, #var2)| {
                        nami::__format!(#format_str)
                    })
                }
            }
            .into()
        }
        3 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            let var3 = &var_idents[2];
            quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    SignalExt::map(
                        zip(zip(#var1.clone(), #var2.clone()), #var3.clone()),
                        |((#var1, #var2), #var3)| {
                            ::nami::__format!(#format_str)
                        }
                    )
                }
            }
            .into()
        }
        4 => {
            let var1 = &var_idents[0];
            let var2 = &var_idents[1];
            let var3 = &var_idents[2];
            let var4 = &var_idents[3];
            quote! {
                {
                    use ::nami::{SignalExt, zip::zip};
                    SignalExt::map(
                        zip(
                            zip(#var1.clone(), #var2.clone()),
                            zip(#var3.clone(), #var4.clone())
                        ),
                        |((#var1, #var2), (#var3, #var4))| {
                            ::nami::__format!(#format_str)
                        }
                    )
                }
            }
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
        if c == '{' && chars.peek() == Some(&'{') {
            // Skip escaped braces
            chars.next();
            continue;
        } else if c == '{' {
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
                } else {
                    content.push(chars.next().unwrap());
                    has_content = true;
                }
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
