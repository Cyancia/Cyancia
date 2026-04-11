use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let static_name =
        format_ident!("__{}_EVENT_CHANNEL", name.to_string().to_uppercase());

    quote! {
        static #static_name: ::cyancia_runtime::__private::LazyLock<(
            ::cyancia_runtime::__private::Sender<#name>,
            ::cyancia_runtime::__private::Receiver<#name>,
        )> = ::cyancia_runtime::__private::LazyLock::new(
            ::cyancia_runtime::__private::unbounded::<#name>,
        );

        impl ::cyancia_runtime::event::Event for #name {
            fn channel() -> &'static ::cyancia_runtime::__private::LazyLock<(
                ::cyancia_runtime::__private::Sender<#name>,
                ::cyancia_runtime::__private::Receiver<#name>,
            )> {
                &#static_name
            }
        }
    }
    .into()
}
