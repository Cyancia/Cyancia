mod deref;

#[proc_macro_derive(Deref, attributes(deref))]
pub fn derive_deref(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    deref::expand(input, false)
}

#[proc_macro_derive(DerefMut, attributes(deref))]
pub fn derive_deref_mut(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    deref::expand(input, true)
}
