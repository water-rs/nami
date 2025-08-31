use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

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
/// let person_binding = binding(Person {
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
        Data::Struct(data_struct) => {
            match &data_struct.fields {
                Fields::Named(fields_named) => {
                    derive_project_struct(&input, fields_named)
                }
                Fields::Unnamed(fields_unnamed) => {
                    derive_project_tuple_struct(&input, fields_unnamed)
                }
                Fields::Unit => {
                    derive_project_unit_struct(&input)
                }
            }
        }
        Data::Enum(_) => {
            syn::Error::new_spanned(
                input,
                "Project derive macro does not support enums"
            )
            .to_compile_error()
            .into()
        }
        Data::Union(_) => {
            syn::Error::new_spanned(
                input,
                "Project derive macro does not support unions"
            )
            .to_compile_error()
            .into()
        }
    }
}

fn derive_project_struct(input: &DeriveInput, fields: &syn::FieldsNamed) -> TokenStream {
    let struct_name = &input.ident;
    let (_impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    
    // Create the projected struct type
    let projected_struct_name = syn::Ident::new(
        &format!("{}Projected", struct_name),
        struct_name.span(),
    );
    
    // Generate fields for the projected struct
    let projected_fields = fields.named.iter().map(|field| {
        let field_name = &field.ident;
        let field_type = &field.ty;
        quote! {
            pub #field_name: nami::Binding<#field_type>
        }
    });
    
    // Generate the projection logic
    let field_projections = fields.named.iter().map(|field| {
        let field_name = &field.ident;
        quote! {
            #field_name: {
                let source = source.clone();
                nami::Binding::mapping(
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
        
        impl #impl_generics_with_static nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = #projected_struct_name #ty_generics;
            
            fn project(source: &nami::Binding<Self>) -> Self::Projected {
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
        quote! { (nami::Binding<#(#field_types)*>,) }
    } else {
        quote! { (#(nami::Binding<#field_types>),*) }
    };
    
    // Generate field projections using index access
    let field_projections = fields.unnamed.iter().enumerate().map(|(index, _)| {
        let idx = syn::Index::from(index);
        quote! {
            {
                let source = source.clone();
                nami::Binding::mapping(
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
        impl #impl_generics_with_static nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = #projected_tuple;
            
            fn project(source: &nami::Binding<Self>) -> Self::Projected {
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
        impl #impl_generics_with_static nami::project::Project for #struct_name #ty_generics #where_clause {
            type Projected = ();
            
            fn project(_source: &nami::Binding<Self>) -> Self::Projected {
                ()
            }
        }
    };
    
    TokenStream::from(expanded)
}