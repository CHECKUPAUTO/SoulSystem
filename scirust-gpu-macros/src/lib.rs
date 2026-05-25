use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, PatType, Type, TypeReference, parse_macro_input};

#[proc_macro_attribute]
pub fn gpu(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let block = &input_fn.block;
    let attrs = &input_fn.attrs;

    let mut found_slice = false;
    for arg in &sig.inputs {
        if let FnArg::Typed(PatType { ty, .. }) = arg {
            if let Type::Reference(TypeReference { elem, mutability, .. }) = ty.as_ref() {
                if mutability.is_some() {
                    if let Type::Slice(slice) = elem.as_ref() {
                        if let Type::Path(path) = &*slice.elem {
                            if let Some(seg_expr) = path.path.segments.first() {
                                if seg_expr.ident == "f32" {
                                    found_slice = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !found_slice {
        let expanded = quote! {
            #(#attrs)*
            #vis #sig #block
        };
        return TokenStream::from(expanded);
    }

    let expanded = quote! {
        #(#attrs)*
        #vis #sig {
            let data = {
                let mut target: Option<&mut [f32]> = None;
                #block
            };
            data
        }
    };

    TokenStream::from(expanded)
}
