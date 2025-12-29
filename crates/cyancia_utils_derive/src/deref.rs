const DEREF_ATTR: &str = "deref";

pub fn expand(input: proc_macro::TokenStream, is_deref_mut: bool) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;
    let data = match &input.data {
        syn::Data::Struct(s) => s,
        _ => panic!("#[derive(Deref)] is only defined for structs"),
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let field = {
        if data.fields.len() == 1 {
            data.fields.iter().next().unwrap()
        } else {
            data.fields
                .iter()
                .find(|f| {
                    f.attrs
                        .iter()
                        .find(|a| a.path().get_ident().unwrap() == DEREF_ATTR)
                        .is_some()
                })
                .expect("Cannot find field with #[deref] attribute")
        }
    };
    let field_ty = &field.ty;
    let access = {
        if let Some(ident) = &field.ident {
            quote::quote! {.#ident}
        } else {
            quote::quote! {.0}
        }
    };

    if is_deref_mut {
        quote::quote! {
            impl #impl_generics std::ops::DerefMut for #name #ty_generics #where_clause {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self #access
                }
            }
        }
    } else {
        quote::quote! {
            impl #impl_generics std::ops::Deref for #name #ty_generics #where_clause {
                type Target = #field_ty;

                fn deref(&self) -> &Self::Target {
                    &self #access
                }
            }
        }
    }
    .into()
}
