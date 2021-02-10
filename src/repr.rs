//! Generates the representation struct.

use proc_macro2::{Ident, TokenStream};
use quote::{quote, format_ident, ToTokens};
use syn::{Abi, BareFnArg, Path, token::Colon};
use crate::{
    attr::StageStash,
    vtable::{VtableFnArg, VtableItem},
};

pub fn generate_repr(
    stash: &mut StageStash,
    inline_vtable: bool,
    path_to_box: Path,
    drop_abi: Option<&Abi>,
) -> TokenStream {
    let StageStash {
        repr_name,
        vtable_name,
        trait_name,
        vtable_items,
        ..
    } = stash;
    let (vtable_contents, thunk_methods) =
        generate_vtable_and_thunks(&repr_name, vtable_items.iter().cloned());

    // Perform necessary branching depending on vtable style in advance.
    let (vtable_field_type, ctor_val) = if inline_vtable {
        // The type of the vtable field is the vtable type's name itself,
        // so just get a token stream of it.
        let vtable_field_type = vtable_name.to_token_stream();
        // The constructor will memcpy the vtable into the repr struct.
        let ctor_val = quote! {
            Self {
                __thintraitobjectmacro_repr_vtable: Self::__THINTRAITOBJECTMACRO_VTABLE,
                __thintraitobjectmacro_repr_value: __thintraitobjectmacro_arg0,
            }
        };
        (vtable_field_type, ctor_val)
    } else {
        // Here, we need to construct a reference-to-static type with the vtable typename.
        let vtable_field_type = quote! {
            &'static #vtable_name
        };
        // The constructor will borrow the static vtable.
        let ctor_val = quote! {
            Self {
                __thintraitobjectmacro_repr_vtable: &Self::__THINTRAITOBJECTMACRO_VTABLE,
                __thintraitobjectmacro_repr_value: __thintraitobjectmacro_arg0,
            }
        };
        (vtable_field_type, ctor_val)
    };
    // Here comes the cluttered part: heavily prefixed names.
    let repr = quote! {
        #[repr(C)]
        struct #repr_name <__ThinTraitObjectMacro_ReprGeneric0: #trait_name> {
            __thintraitobjectmacro_repr_vtable: #vtable_field_type,
            __thintraitobjectmacro_repr_value: __ThinTraitObjectMacro_ReprGeneric0,
        }
        impl<
            __ThinTraitObjectMacro_ReprGeneric0: #trait_name
        > #repr_name<__ThinTraitObjectMacro_ReprGeneric0> {
            const __THINTRAITOBJECTMACRO_VTABLE: #vtable_name = #vtable_name {
                #vtable_contents
                drop: Self :: __thintraitobjectmacro_repr_drop,
            };

            fn __thintraitobjectmacro_repr_create(
                __thintraitobjectmacro_arg0: __ThinTraitObjectMacro_ReprGeneric0,
            ) -> *mut #vtable_name {
                #path_to_box::into_raw(#path_to_box::new(#ctor_val)) as *mut _
            }
            // Simple destructor which uses Box's internals to deallocate and
            // drop the value as necessary.
            unsafe #drop_abi fn __thintraitobjectmacro_repr_drop(
                __thintraitobjectmacro_arg0: *mut ::core::ffi::c_void,
            ) {
                let _ = #path_to_box::from_raw(
                    __thintraitobjectmacro_arg0
                        as *mut #repr_name<__ThinTraitObjectMacro_ReprGeneric0>
                );
            }
            #thunk_methods
        }
    };
    repr
}

#[inline]
pub fn repr_name_from_trait_name(trait_name: Ident) -> Ident {
    format_ident!("__ThinTraitObjectMacro_ReprFor{}", trait_name)
}

fn generate_vtable_and_thunks(
    repr_name: &Ident,
    vtable_entries: impl IntoIterator<Item = VtableItem>,
) -> (TokenStream, TokenStream) {
    let mut vtable_contents = TokenStream::new();
    let mut thunk_methods = TokenStream::new();
    for mut entry in vtable_entries {
        entry.make_raw();
        entry.make_unsafe();
        // Create the list of arguments decorated with the collision-avoiding
        // names. Using mixed-site hygeine could be a better solution.
        let mut argument_counter = 1_u32;
        let thunk_call_args = entry.inputs.clone().into_iter().skip(1).map(|x| {
            let arg = to_nth_thunk_arg(x, argument_counter);
            argument_counter += 1;
            arg
        });

        // Clone this out before handing them over to to_signature().
        let name = entry.name.clone();

        let thunk_name = format_ident!("__thintraitobjectmacro_thunk_{}", &entry.name);
        let thunk_signature = {
            let mut signature = entry.into_signature(nth_arg);
            signature.ident = thunk_name.clone();
            signature
        };

        // Remember that this gets called in a loop, so we add one vtable
        // constructor entry for every vtable entry.
        (quote! {
            #name: Self :: #thunk_name,
        })
        .to_tokens(&mut vtable_contents);

        // Generate the thunks, again, one for every vtable entry. Those are
        // pretty simple, actually: just unsafely convert the pointer to a
        // reference to the repr struct and call the appropriate method,
        // offsetting into the actual value.
        (quote! {
            #thunk_signature {
                (
                    *(__thintraitobjectmacro_arg0
                        as *mut #repr_name<__ThinTraitObjectMacro_ReprGeneric0>
                    )
                ).__thintraitobjectmacro_repr_value.#name(#(#thunk_call_args)*)
            }
        })
        .to_tokens(&mut thunk_methods);
    }
    (vtable_contents, thunk_methods)
}

fn nth_arg(n: u32) -> Ident {
    format_ident!("__thintraitobjectmacro_arg{}", n)
}
/// Transforms a VtableFnArg to an argument to a thunk.
fn to_nth_thunk_arg(arg: VtableFnArg, n: u32) -> BareFnArg {
    let mut arg = arg.into_bare_arg_with_ptr_receiver();
    arg.name = Some(arg.name.unwrap_or_else(|| (nth_arg(n), Colon::default())));
    arg
}