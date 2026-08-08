//! Generates the `Validate` impls that check a payload against the schema
//! constraints the generated *types* cannot carry.
//!
//! Most of the specification is already in the type: a property bounded at
//! `maxLength: 20` becomes a `heapless::String<20>`, and no over-long value
//! can exist. This module covers only what is left over, and deliberately
//! emits nothing where the type already decides the question -- a check
//! that cannot fail is pure code size in a crate whose users count bytes.
//!
//! Two things are left over:
//!
//! * **Bounds the type deliberately does not hold.** A string over
//!   [`crate::schema`]'s inline threshold, or an array over its own, is not
//!   stored at the spec's ceiling: under `alloc` it is a growable
//!   `String`/`Vec` with no bound at all, and without `alloc` it is a
//!   `heapless` collection at a capacity the *caller* chose, which may sit
//!   either side of the spec's. `AuthorizeRequest::certificate` is the
//!   canonical one -- `maxLength: 5500`, stored as a plain `String`.
//! * **Constraints no collection type expresses.** `minItems`,
//!   `minimum`/`maximum`, and 1.6J's `multipleOf`. A `heapless::Vec<T, N>`
//!   says "at most N" and nothing else; "at least one" has to be checked.
//!
//! Every struct and enum gets an impl even when it has nothing to check, so
//! a parent can recurse into a field without knowing whether that type
//! cares. The empty ones cost nothing at runtime.

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;

use crate::model::*;
use crate::rust::MAX_STACK_VEC_CAPACITY;

/// Which `alloc` build an item belongs to, and so what `#[cfg]` it needs.
///
/// A struct with a caller-sized field is emitted twice under opposite
/// gates; everything else is emitted once, ungated. The `Validate` impl has
/// to repeat whichever gate its struct got -- and add its own feature on
/// top -- or the two bodies' impls overlap.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// One definition, compiled in both builds.
    Always,
    Alloc,
    NoAlloc,
}

impl Gate {
    /// The attribute for the struct and its `Action` impl.
    pub fn item_attr(self) -> TokenStream {
        match self {
            Self::Always => quote! {},
            Self::Alloc => quote! { #[cfg(feature = "alloc")] },
            Self::NoAlloc => quote! { #[cfg(not(feature = "alloc"))] },
        }
    }

    /// The attribute for the `Validate` impl: this item's `alloc` gate, and
    /// the `validate` feature as well.
    fn validate_attr(self) -> TokenStream {
        match self {
            Self::Always => quote! { #[cfg(feature = "validate")] },
            Self::Alloc => quote! { #[cfg(all(feature = "validate", feature = "alloc"))] },
            Self::NoAlloc => {
                quote! { #[cfg(all(feature = "validate", not(feature = "alloc")))] }
            }
        }
    }
}

/// How to name the value being checked, in the three positions the checks
/// need it. Which one applies depends on where the value came from -- a
/// field of `self`, the binding of an `if let Some(..)`, or an array's
/// loop variable -- so every check site takes one of these rather than
/// reconstructing the expression.
struct Access {
    /// The value as a place expression, for method calls: `self.foo`.
    place: TokenStream,
    /// A shared reference to it: `&self.foo`.
    reference: TokenStream,
    /// The value itself, for `Copy` scalars: `self.foo`, or `*item`.
    scalar: TokenStream,
}

/// Renders `impl Validate for #name`, checking every field of `model` that
/// needs it and recursing into the ones that hold other generated types.
pub fn generate_struct_impl(
    model: &RustStruct,
    impl_generics: &TokenStream,
    type_generics: &TokenStream,
    alloc_mode: bool,
    gate: Gate,
) -> TokenStream {
    let name = format_ident!("{}", model.name);
    let attr = gate.validate_attr();

    let body = model
        .fields
        .iter()
        .map(|field| field_statements(field, alloc_mode));

    quote! {
        #attr
        impl #impl_generics crate::validate::Validate for #name #type_generics {
            fn validate(&self) -> Result<(), crate::validate::ValidationError> {
                #(#body)*

                Ok(())
            }
        }
    }
}

/// Renders the impl for a generated enum. A schema enum is a closed set of
/// string values, every one of which is by definition conformant -- but the
/// impl still has to exist, since a struct holding one recurses into it
/// like any other named type.
pub fn generate_enum_impl(model: &RustEnum) -> TokenStream {
    let name = format_ident!("{}", model.name);
    let attr = Gate::Always.validate_attr();

    quote! {
        #attr
        impl crate::validate::Validate for #name {
            fn validate(&self) -> Result<(), crate::validate::ValidationError> {
                Ok(())
            }
        }
    }
}

fn field_statements(field: &RustField, alloc_mode: bool) -> TokenStream {
    let ident = crate::rust::field_ident(&crate::naming::rust_name(&field.name));
    let wire_name = &field.name;
    let path = vec![quote! { in_field(#wire_name) }];

    // An optional field binds its contents so the checks read the same as a
    // required one's. Scalars bind by value (they are `Copy`, and the
    // numeric checks take them by value); everything else by reference.
    let scalar_field = matches!(field.ty, RustType::Integer | RustType::Number);
    let access = if field.optional {
        Access {
            place: quote! { value },
            reference: quote! { value },
            scalar: quote! { value },
        }
    } else {
        Access {
            place: quote! { self.#ident },
            reference: quote! { &self.#ident },
            scalar: quote! { self.#ident },
        }
    };

    let statements = checks(&access, &field.ty, &field.constraints, &path, 0, alloc_mode);

    if statements.is_empty() {
        return quote! {};
    }

    if field.optional {
        let bound = if scalar_field {
            quote! { self.#ident }
        } else {
            quote! { &self.#ident }
        };

        return quote! {
            if let Some(value) = #bound {
                #(#statements)*
            }
        };
    }

    quote! { #(#statements)* }
}

/// The checks for one value: its own constraints, then -- for a named type
/// or an array -- whatever its contents need.
///
/// `path` names the value from the message root outward, one entry per
/// segment, each already rendered as the `ValidationError` call that
/// prepends it. `depth` counts enclosing arrays, so nested loops don't
/// shadow each other's bindings.
fn checks(
    access: &Access,
    ty: &RustType,
    constraints: &Constraints,
    path: &[TokenStream],
    depth: usize,
    alloc_mode: bool,
) -> Vec<TokenStream> {
    let mut statements = Vec::new();
    let attribute = attribute_path(path);
    // The array cases work off `access` as a whole, since they hand it to
    // `array_checks`; the scalar ones only need one form of it each.
    let Access {
        reference, scalar, ..
    } = access;

    match ty {
        // The type holds no bound of its own, so the spec's applies here.
        RustType::UnboundedString(_) => {
            if let Some(max) = constraints.max_length {
                statements.push(quote! {
                    crate::validate::check_max_length(#reference, #max) #attribute ?;
                });
            }
        }

        RustType::Integer => {
            statements.extend(numeric_checks(scalar, constraints, &attribute, true));
        }

        RustType::Number => {
            statements.extend(numeric_checks(scalar, constraints, &attribute, false));
        }

        RustType::Local(_) => {
            // Fully qualified: generated modules import their `common`
            // types with a glob and never import the trait itself.
            statements.push(quote! {
                crate::validate::Validate::validate(#reference) #attribute ?;
            });
        }

        RustType::Vec(item, capacity) => {
            // A spec-bounded array is stored at exactly its `maxItems`
            // unless it was too large to inline, in which case the `alloc`
            // build swapped it for a growable `Vec` that no longer holds
            // the bound.
            let type_holds_max = !(alloc_mode && *capacity > MAX_STACK_VEC_CAPACITY);
            statements.extend(array_checks(
                access,
                item,
                constraints,
                path,
                depth,
                alloc_mode,
                type_holds_max,
            ));
        }

        // A caller-chosen capacity is never the spec's bound, in either
        // build: under `alloc` there is none, and without it the caller may
        // have picked a capacity above the spec's.
        RustType::UnboundedVec(item, _) => {
            statements.extend(array_checks(
                access, item, constraints, path, depth, alloc_mode, false,
            ));
        }

        // `BoundedString` is stored at exactly its `maxLength`, and a byte
        // capacity of N admits at most N characters, so the spec's limit
        // holds by construction. `Primitive`/`CrateType` are hand-written
        // types that enforce their own format. `Any` is the caller's
        // payload, which the spec does not describe. The rest carry no
        // constraints at all.
        RustType::BoundedString(_)
        | RustType::Bool
        | RustType::CrateType(_)
        | RustType::Primitive(_)
        | RustType::Any(_)
        | RustType::Unknown => {}
    }

    statements
}

#[allow(clippy::too_many_arguments)]
fn array_checks(
    access: &Access,
    item_ty: &RustType,
    constraints: &Constraints,
    path: &[TokenStream],
    depth: usize,
    alloc_mode: bool,
    type_holds_max: bool,
) -> Vec<TokenStream> {
    let mut statements = Vec::new();
    let attribute = attribute_path(path);
    let place = &access.place;

    // `len() < 0` is not a thing; a `minItems: 0` states nothing.
    if let Some(min) = constraints.min_items.filter(|min| *min > 0) {
        statements.push(quote! {
            crate::validate::check_min_items(#place.len(), #min) #attribute ?;
        });
    }

    if !type_holds_max
        && let Some(max) = constraints.max_items
    {
        statements.push(quote! {
            crate::validate::check_max_items(#place.len(), #max) #attribute ?;
        });
    }

    let index = loop_ident("index", depth);
    let item = loop_ident("item", depth);

    let mut item_path = path.to_vec();
    item_path.push(quote! { in_index(#index) });

    let item_constraints = constraints.item.clone().unwrap_or_default();
    let item_access = Access {
        place: quote! { #item },
        reference: quote! { #item },
        // The loop binding is a reference, so a scalar element needs
        // dereferencing before the numeric checks will take it.
        scalar: quote! { *#item },
    };

    let item_statements = checks(
        &item_access,
        item_ty,
        &item_constraints,
        &item_path,
        depth + 1,
        alloc_mode,
    );

    if !item_statements.is_empty() {
        statements.push(quote! {
            for (#index, #item) in #place.iter().enumerate() {
                #(#item_statements)*
            }
        });
    }

    statements
}

/// `minimum`/`maximum`/`multipleOf` for one numeric value.
///
/// An integer field is compared as an integer where the bound is a whole
/// number (which every one in the schemas is), so the decision never goes
/// through a float. `multipleOf` has no integer form -- the only ones in
/// any version are 1.6J's `0.1` -- so it always does.
fn numeric_checks(
    value: &TokenStream,
    constraints: &Constraints,
    attribute: &TokenStream,
    integer: bool,
) -> Vec<TokenStream> {
    let mut statements = Vec::new();

    if let Some(min) = constraints.minimum {
        statements.push(match whole(min).filter(|_| integer) {
            Some(min) => quote! {
                crate::validate::check_min_i64(#value, #min) #attribute ?;
            },
            None if integer => quote! {
                crate::validate::check_min_f64(#value as f64, #min) #attribute ?;
            },
            None => quote! {
                crate::validate::check_min_f64(#value, #min) #attribute ?;
            },
        });
    }

    if let Some(max) = constraints.maximum {
        statements.push(match whole(max).filter(|_| integer) {
            Some(max) => quote! {
                crate::validate::check_max_i64(#value, #max) #attribute ?;
            },
            None if integer => quote! {
                crate::validate::check_max_f64(#value as f64, #max) #attribute ?;
            },
            None => quote! {
                crate::validate::check_max_f64(#value, #max) #attribute ?;
            },
        });
    }

    if let Some(multiple) = constraints.multiple_of {
        let value = if integer {
            quote! { #value as f64 }
        } else {
            value.clone()
        };
        statements.push(quote! {
            crate::validate::check_multiple_of(#value, #multiple) #attribute ?;
        });
    }

    statements
}

/// The bound as an `i64`, when it is a whole number an `i64` represents
/// exactly. `None` for a fractional bound, which has to stay in floating
/// point.
fn whole(value: f64) -> Option<i64> {
    let truncated = value as i64;

    (truncated as f64 == value).then_some(truncated)
}

/// The `map_err` that turns a check's root-level error into one carrying
/// the path to this value.
///
/// `path` runs outermost-first and the error is built by prepending, so the
/// calls are chained in reverse: the innermost segment goes on first.
fn attribute_path(path: &[TokenStream]) -> TokenStream {
    let segments = path.iter().rev();

    quote! { .map_err(|error| error #(. #segments)*) }
}

/// Loop bindings are suffixed by nesting depth so an array of arrays does
/// not shadow the index its own error path refers to.
fn loop_ident(base: &str, depth: usize) -> proc_macro2::Ident {
    if depth == 0 {
        format_ident!("{}", base)
    } else {
        format_ident!("{}_{}", base, depth)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::rust::generate;

    /// prettyplease decides its own line breaks, so every assertion here
    /// compares against a whitespace-collapsed copy of the output.
    fn flat(code: &str) -> String {
        code.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn schema_with(field: RustField) -> ParsedSchema {
        ParsedSchema {
            message: RustStruct {
                name: "SampleRequest".to_string(),
                action: Some("Sample".to_string()),
                description: None,
                fields: vec![field],
            },
            types: Vec::new(),
        }
    }

    fn field(name: &str, ty: RustType, constraints: Constraints) -> RustField {
        RustField {
            name: name.to_string(),
            ty,
            optional: false,
            description: None,
            constraints,
        }
    }

    fn cap(name: &str, default: usize) -> ConstParam {
        ConstParam {
            name: name.to_string(),
            default,
        }
    }

    #[test]
    fn a_struct_with_nothing_to_check_still_gets_an_impl() {
        // Uniformity is what makes recursion work: a parent validates a
        // nested field by calling `validate` on it, without knowing whether
        // that type has anything of its own to check.
        let code = generate(schema_with(field(
            "status",
            RustType::Bool,
            Constraints::default(),
        )));

        assert!(
            flat(&code).contains("impl crate::validate::Validate for SampleRequest"),
            "{code}"
        );
    }

    #[test]
    fn the_impl_is_gated_behind_the_validate_feature() {
        let code = generate(schema_with(field(
            "status",
            RustType::Bool,
            Constraints::default(),
        )));

        assert!(flat(&code).contains("#[cfg(feature = \"validate\")]"), "{code}");
    }

    #[test]
    fn a_string_too_large_to_inline_is_checked_against_its_spec_max_length() {
        // The case the whole module exists for: `maxLength: 5500` is real,
        // but the field is a growable `String` under `alloc` and a
        // caller-sized `heapless::String` without it, so neither build's
        // type enforces the spec's number.
        let code = generate(schema_with(field(
            "certificate",
            RustType::UnboundedString(cap("SAMPLE_REQUEST_CERTIFICATE_CAP", 1024)),
            Constraints {
                max_length: Some(5500),
                ..Constraints::default()
            },
        )));

        assert!(
            flat(&code).contains(
                "crate::validate::check_max_length(&self.certificate, 5500usize) \
                 .map_err(|error| error.in_field(\"certificate\"))?;"
            ),
            "{code}"
        );
    }

    #[test]
    fn a_string_the_spec_never_bounds_is_not_checked() {
        let code = generate(schema_with(field(
            "data",
            RustType::UnboundedString(cap("SAMPLE_REQUEST_DATA_CAP", 1024)),
            Constraints::default(),
        )));

        assert!(!code.contains("check_max_length"), "{code}");
    }

    #[test]
    fn an_inline_bounded_string_is_not_checked_because_its_type_already_bounds_it() {
        // A `heapless::String<20>` cannot hold 21 bytes, and 20 bytes is at
        // most 20 characters, so the spec's `maxLength: 20` holds by
        // construction. Emitting the check would cost every field a
        // comparison that cannot fail.
        let code = generate(schema_with(field(
            "reasonCode",
            RustType::BoundedString(20),
            Constraints {
                max_length: Some(20),
                ..Constraints::default()
            },
        )));

        assert!(!code.contains("check_max_length"), "{code}");
    }

    #[test]
    fn min_items_is_checked_because_no_collection_type_can_express_it() {
        let code = generate(schema_with(field(
            "meterValue",
            RustType::Vec(Box::new(RustType::Local("MeterValue".to_string())), 4),
            Constraints {
                min_items: Some(1),
                max_items: Some(4),
                ..Constraints::default()
            },
        )));

        assert!(
            flat(&code).contains(
                "crate::validate::check_min_items(self.meter_value.len(), 1usize) \
                 .map_err(|error| error.in_field(\"meterValue\"))?;"
            ),
            "{code}"
        );
    }

    #[test]
    fn max_items_of_an_inlined_array_is_not_checked() {
        // `heapless::Vec<T, 4>` already cannot hold five.
        let code = generate(schema_with(field(
            "meterValue",
            RustType::Vec(Box::new(RustType::Local("MeterValue".to_string())), 4),
            Constraints {
                max_items: Some(4),
                ..Constraints::default()
            },
        )));

        assert!(!code.contains("check_max_items"), "{code}");
    }

    #[test]
    fn max_items_of_a_caller_sized_array_is_checked() {
        // A capacity the caller picks can sit either side of the spec's, and
        // under `alloc` the array is unbounded outright.
        let code = generate(schema_with(field(
            "chargingSchedulePeriod",
            RustType::UnboundedVec(
                Box::new(RustType::Local("ChargingSchedulePeriod".to_string())),
                cap("SAMPLE_REQUEST_CHARGING_SCHEDULE_PERIOD_CAP", 8),
            ),
            Constraints {
                max_items: Some(1024),
                ..Constraints::default()
            },
        )));

        assert!(
            flat(&code).contains(
                "crate::validate::check_max_items(self.charging_schedule_period.len(), 1024usize)"
            ),
            "{code}"
        );
    }

    #[test]
    fn a_nested_struct_is_recursed_into_under_its_wire_name() {
        let code = generate(schema_with(field(
            "idToken",
            RustType::Local("IdToken".to_string()),
            Constraints::default(),
        )));

        assert!(
            flat(&code).contains(
                "crate::validate::Validate::validate(&self.id_token) \
                 .map_err(|error| error.in_field(\"idToken\"))?;"
            ),
            "{code}"
        );
    }

    #[test]
    fn array_items_are_recursed_into_and_report_their_index() {
        let code = generate(schema_with(field(
            "meterValue",
            RustType::Vec(Box::new(RustType::Local("MeterValue".to_string())), 4),
            Constraints::default(),
        )));

        let flat = flat(&code);
        assert!(
            flat.contains("for (index, item) in self.meter_value.iter().enumerate()"),
            "{code}"
        );
        assert!(
            flat.contains(
                "crate::validate::Validate::validate(item) \
                 .map_err(|error| error.in_index(index).in_field(\"meterValue\"))?;"
            ),
            "{code}"
        );
    }

    #[test]
    fn an_optional_field_is_only_checked_when_it_is_present() {
        let mut sample = field(
            "certificate",
            RustType::UnboundedString(cap("SAMPLE_REQUEST_CERTIFICATE_CAP", 1024)),
            Constraints {
                max_length: Some(5500),
                ..Constraints::default()
            },
        );
        sample.optional = true;

        let code = generate(schema_with(sample));

        let flat = flat(&code);
        assert!(
            flat.contains("if let Some(value) = &self.certificate"),
            "{code}"
        );
        assert!(
            flat.contains("crate::validate::check_max_length(value, 5500usize)"),
            "{code}"
        );
    }

    #[test]
    fn integer_bounds_are_compared_as_integers_not_through_floating_point() {
        let code = generate(schema_with(field(
            "evseId",
            RustType::Integer,
            Constraints {
                minimum: Some(0.0),
                maximum: Some(9.0),
                ..Constraints::default()
            },
        )));

        let flat = flat(&code);
        assert!(
            flat.contains("crate::validate::check_min_i64(self.evse_id, 0i64)"),
            "{code}"
        );
        assert!(
            flat.contains("crate::validate::check_max_i64(self.evse_id, 9i64)"),
            "{code}"
        );
    }

    #[test]
    fn number_bounds_and_steps_use_the_float_checks() {
        let code = generate(schema_with(field(
            "limit",
            RustType::Number,
            Constraints {
                minimum: Some(0.0),
                multiple_of: Some(0.1),
                ..Constraints::default()
            },
        )));

        let flat = flat(&code);
        assert!(
            flat.contains("crate::validate::check_min_f64(self.limit, 0f64)"),
            "{code}"
        );
        assert!(
            flat.contains("crate::validate::check_multiple_of(self.limit, 0.1f64)"),
            "{code}"
        );
    }

    #[test]
    fn a_caller_chosen_payload_type_is_not_required_to_implement_validate() {
        // `customData` and 2.x's `DataTransfer.data` are the caller's types
        // and the spec constrains neither, so requiring `Validate` of them
        // would tax every user for nothing.
        let mut custom_data = field(
            "customData",
            RustType::Any(TypeParam {
                name: "CustomDataType".to_string(),
                default: "crate::NoCustomData".to_string(),
            }),
            Constraints::default(),
        );
        custom_data.optional = true;

        let code = generate(schema_with(custom_data));

        assert!(
            flat(&code)
                .contains("impl<CustomDataType> crate::validate::Validate for SampleRequest<CustomDataType>"),
            "{code}"
        );
        assert!(!code.contains("CustomDataType: crate::validate::Validate"), "{code}");
        assert!(!code.contains("self.custom_data"), "{code}");
    }

    #[test]
    fn an_enum_gets_a_trivial_impl_so_references_to_it_resolve() {
        let mut schema = schema_with(field(
            "status",
            RustType::Local("StatusEnum".to_string()),
            Constraints::default(),
        ));
        schema.types.push(GeneratedType::Enum(RustEnum {
            name: "StatusEnum".to_string(),
            description: None,
            variants: vec![RustVariant {
                ident: "Accepted".to_string(),
                raw: "Accepted".to_string(),
            }],
        }));

        let code = generate(schema);

        assert!(
            flat(&code).contains("impl crate::validate::Validate for StatusEnum"),
            "{code}"
        );
    }

    #[test]
    fn each_alloc_variant_of_a_split_struct_gets_its_own_gated_impl() {
        // A struct with a caller-sized field is emitted twice, under
        // opposite `alloc` gates. Their `Validate` impls have to be gated
        // the same way, or the two collide.
        let code = generate(schema_with(field(
            "certificate",
            RustType::UnboundedString(cap("SAMPLE_REQUEST_CERTIFICATE_CAP", 1024)),
            Constraints {
                max_length: Some(5500),
                ..Constraints::default()
            },
        )));

        let flat = flat(&code);
        assert!(
            flat.contains("#[cfg(all(feature = \"validate\", feature = \"alloc\"))]"),
            "{code}"
        );
        assert!(
            flat.contains("#[cfg(all(feature = \"validate\", not(feature = \"alloc\")))]"),
            "{code}"
        );
    }

    #[test]
    fn a_caller_sized_string_impl_carries_the_structs_const_params() {
        let code = generate(schema_with(field(
            "certificate",
            RustType::UnboundedString(cap("SAMPLE_REQUEST_CERTIFICATE_CAP", 1024)),
            Constraints {
                max_length: Some(5500),
                ..Constraints::default()
            },
        )));

        assert!(
            flat(&code).contains(
                "impl<const SAMPLE_REQUEST_CERTIFICATE_CAP: usize> crate::validate::Validate \
                 for SampleRequest<SAMPLE_REQUEST_CERTIFICATE_CAP>"
            ),
            "{code}"
        );
    }

    #[test]
    fn string_items_inside_an_array_are_checked_against_the_items_own_bound() {
        let code = generate(schema_with(field(
            "certificateChain",
            RustType::UnboundedVec(
                Box::new(RustType::UnboundedString(cap(
                    "SAMPLE_REQUEST_CERTIFICATE_CHAIN_ITEM_CAP",
                    1024,
                ))),
                cap("SAMPLE_REQUEST_CERTIFICATE_CHAIN_CAP", 8),
            ),
            Constraints {
                item: Some(Box::new(Constraints {
                    max_length: Some(5500),
                    ..Constraints::default()
                })),
                ..Constraints::default()
            },
        )));

        assert!(
            flat(&code).contains(
                "crate::validate::check_max_length(item, 5500usize) \
                 .map_err(|error| error.in_index(index).in_field(\"certificateChain\"))?;"
            ),
            "{code}"
        );
    }
}
