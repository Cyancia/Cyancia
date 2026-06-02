use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, ItemImpl};

#[proc_macro_attribute]
pub fn stateless(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let crate_path: TokenStream2 =
        if std::env::var("CARGO_PKG_NAME").as_deref() == Ok("cyancia_shader_graph") {
            quote! { crate }
        } else {
            quote! { ::cyancia_shader_graph }
        };

    let impl_block = parse_macro_input!(item as ItemImpl);
    let graph_node_impl = generate_graph_node_impl(&impl_block, &crate_path);

    quote! {
        #impl_block
        #graph_node_impl
    }
    .into()
}

fn generate_graph_node_impl(impl_block: &ItemImpl, crate_path: &TokenStream2) -> TokenStream2 {
    let self_ty = &impl_block.self_ty;

    let generics = &impl_block.generics;
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let data_ty: TokenStream2 = impl_block
        .trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last())
        .and_then(|seg| {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                args.args.first().cloned()
            } else {
                None
            }
        })
        .and_then(|arg| {
            if let syn::GenericArgument::Type(ty) = arg {
                Some(quote! { #ty })
            } else {
                None
            }
        })
        .unwrap_or_else(|| quote! { Data });

    quote! {
        impl #impl_generics #crate_path::graph::node::GraphNode<#data_ty>
            for #self_ty
            #where_clause
        {
            type State = #crate_path::graph::node::StatelessState;

            fn name(&self) -> &'static str {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::name(self)
            }

            fn default_state(&self) -> Self::State {
                #crate_path::graph::node::StatelessState::default()
            }

            fn header_color(&self, cx: &::gpui::App) -> gpui::Rgba {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::header_color(self, cx)
            }

            fn create_inputs(
                &self,
                _state: &Self::State,
                ctx: #crate_path::graph::node::GraphNodeCreateSlotsContext<'_, #data_ty>,
            ) -> ::std::vec::Vec<#crate_path::graph::slot::GraphDefaultInputSlot> {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::create_inputs(self, ctx)
            }

            fn create_outputs(
                &self,
                _state: &Self::State,
                ctx: #crate_path::graph::node::GraphNodeCreateSlotsContext<'_, #data_ty>,
            ) -> ::std::vec::Vec<#crate_path::graph::slot::GraphDefaultOutputSlot> {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::create_outputs(self, ctx)
            }

            fn render(
                &self,
                _state: &Self::State,
                mut ctx: #crate_path::graph::node::GraphNodeRenderContext<'_, '_, #data_ty>,
            ) -> gpui::AnyElement {
                ctx.render_all_slots()
            }

            fn generate_code(
                &self,
                _state: &Self::State,
                ctx: #crate_path::graph::node::GraphNodeCodeGenContext<'_, #data_ty>,
            ) -> ::std::result::Result<
                ::std::string::String,
                #crate_path::graph::node::GraphNodeCodeGenError,
            > {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::generate_code(self, ctx)
            }

            fn run(
                &self,
                _state: &Self::State,
                ctx: #crate_path::graph::node::GraphNodeRunContext<'_, #data_ty>,
            ) -> ::std::result::Result<(), #crate_path::graph::node::GraphNodeRunError> {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::run(self, ctx)
            }

            fn update_signature(
                &self,
                _state: &Self::State,
                ctx: #crate_path::graph::node::GraphNodeUpdateSignatureContext<'_, #data_ty>
            ) {
                <Self as #crate_path::graph::node::StatelessCommonGraphNode<#data_ty>>::update_signature(self, ctx);
            }
        }
    }
}
