use crate::model::ConstParam;
use crate::model::GeneratedType;
use crate::model::RustField;
use crate::model::RustType;
use crate::model::TypeParam;
use std::collections::HashMap;

/// For every struct in `types`, computes the full set of const generic
/// parameters it needs to expose in the no-`alloc` build: its own direct
/// `UnboundedString`/`UnboundedVec` fields, plus (transitively) every param
/// required by any type it references through `Local` -- possibly through
/// a `Vec`. Enums never carry params (unit variants only).
///
/// This has to see the whole type graph at once, the same way
/// [`crate::eqcheck::compute_eq_derivable`] does: a struct's own field list
/// doesn't say whether a nested struct three levels down has an unbounded
/// field, so each struct's params can only be known once every other
/// struct's params are known.
pub fn compute_const_params(types: &[GeneratedType]) -> HashMap<String, Vec<ConstParam>> {
    let mut params: HashMap<String, Vec<ConstParam>> = types
        .iter()
        .map(|ty| {
            let name = match ty {
                GeneratedType::Struct(s) => s.name.clone(),
                GeneratedType::Enum(e) => e.name.clone(),
            };
            (name, Vec::new())
        })
        .collect();

    // Monotonically growing (params are only ever added, never removed),
    // so this converges: the total number of distinct params across the
    // whole pool is finite.
    loop {
        let mut changed = false;

        for ty in types {
            if let GeneratedType::Struct(s) = ty {
                let resolved = params_for_fields(&s.fields, &params);
                if resolved != params[&s.name] {
                    params.insert(s.name.clone(), resolved);
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    params
}

/// Resolves the const params a set of fields needs, given the current
/// (possibly not-yet-fully-converged) params of every `Local`-referenced
/// struct. Used both inside the [`compute_const_params`] fixed point and,
/// once that's converged, to resolve a message's own params (messages
/// aren't part of the shared pool `compute_const_params` operates over).
pub fn params_for_fields(
    fields: &[RustField],
    params: &HashMap<String, Vec<ConstParam>>,
) -> Vec<ConstParam> {
    let mut out = Vec::new();
    for field in fields {
        collect_params(&field.ty, params, &mut out);
    }
    out
}

fn collect_params(ty: &RustType, params: &HashMap<String, Vec<ConstParam>>, out: &mut Vec<ConstParam>) {
    match ty {
        RustType::UnboundedString(param) => push_unique(out, param.clone()),
        RustType::UnboundedVec(inner, param) => {
            push_unique(out, param.clone());
            collect_params(inner, params, out);
        }
        RustType::Vec(inner, _) => collect_params(inner, params, out),
        RustType::Local(name) => {
            if let Some(existing) = params.get(name) {
                for param in existing.clone() {
                    push_unique(out, param);
                }
            }
        }
        _ => {}
    }
}

fn push_unique(out: &mut Vec<ConstParam>, param: ConstParam) {
    if !out.iter().any(|existing| existing.name == param.name) {
        out.push(param);
    }
}

/// The [`compute_const_params`] fixed point, for generic *type* parameters
/// ([`RustType::Any`] -- properties the schema leaves untyped) instead of
/// const ones. Kept as a separate pass rather than folded into the const
/// one because the two are rendered differently: type params exist in both
/// the `alloc` and no-`alloc` builds (a caller-chosen payload type means
/// the same thing either way), while const params collapse away under
/// `alloc`.
pub fn compute_type_params(types: &[GeneratedType]) -> HashMap<String, Vec<TypeParam>> {
    let mut params: HashMap<String, Vec<TypeParam>> = types
        .iter()
        .map(|ty| {
            let name = match ty {
                GeneratedType::Struct(s) => s.name.clone(),
                GeneratedType::Enum(e) => e.name.clone(),
            };
            (name, Vec::new())
        })
        .collect();

    loop {
        let mut changed = false;

        for ty in types {
            if let GeneratedType::Struct(s) = ty {
                let resolved = type_params_for_fields(&s.fields, &params);
                if resolved != params[&s.name] {
                    params.insert(s.name.clone(), resolved);
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    params
}

/// [`params_for_fields`], for type params. See [`compute_type_params`].
pub fn type_params_for_fields(
    fields: &[RustField],
    params: &HashMap<String, Vec<TypeParam>>,
) -> Vec<TypeParam> {
    let mut out = Vec::new();
    for field in fields {
        collect_type_params(&field.ty, params, &mut out);
    }
    order_custom_data_last(&mut out);
    out
}

/// Moves the shared `customData` parameter to the end of a parameter list.
///
/// It is introduced by a field that sorts early in most structs, so left in
/// traversal order it would come first and shift every other parameter along
/// -- turning `DataTransferRequest<VendorPayload>` into a binding for
/// `customData` instead of for `data`. Last is also the right place on
/// merit: it is the parameter callers override least often.
pub fn order_custom_data_last(params: &mut Vec<TypeParam>) {
    if let Some(index) = params
        .iter()
        .position(|p| p.name == crate::naming::CUSTOM_DATA_PARAM)
    {
        let param = params.remove(index);
        params.push(param);
    }
}

fn collect_type_params(
    ty: &RustType,
    params: &HashMap<String, Vec<TypeParam>>,
    out: &mut Vec<TypeParam>,
) {
    match ty {
        RustType::Any(param) => push_unique_type(out, param.clone()),
        RustType::Vec(inner, _) => collect_type_params(inner, params, out),
        RustType::UnboundedVec(inner, _) => collect_type_params(inner, params, out),
        RustType::Local(name) => {
            if let Some(existing) = params.get(name) {
                for param in existing.clone() {
                    push_unique_type(out, param);
                }
            }
        }
        _ => {}
    }
}

fn push_unique_type(out: &mut Vec<TypeParam>, param: TypeParam) {
    if !out.iter().any(|existing| existing.name == param.name) {
        out.push(param);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RustStruct;

    fn struct_with_field(name: &str, field_name: &str, ty: RustType) -> GeneratedType {
        GeneratedType::Struct(RustStruct {
            name: name.to_string(),
            action: None,
            description: None,
            fields: vec![RustField {
                name: field_name.to_string(),
                ty,
                optional: false,
                description: None,
            }],
        })
    }

    fn param(name: &str, default: usize) -> ConstParam {
        ConstParam {
            name: name.to_string(),
            default,
        }
    }

    #[test]
    fn struct_with_no_unbounded_field_gets_no_params() {
        let types = vec![struct_with_field("Plain", "value", RustType::Integer)];

        let params = compute_const_params(&types);

        assert!(params["Plain"].is_empty());
    }

    #[test]
    fn struct_with_a_direct_unbounded_field_gets_its_own_param() {
        let types = vec![struct_with_field(
            "Leaf",
            "note",
            RustType::UnboundedString(param("LeafNoteCap", 1024)),
        )];

        let params = compute_const_params(&types);

        assert_eq!(params["Leaf"], vec![param("LeafNoteCap", 1024)]);
    }

    #[test]
    fn struct_referencing_a_generic_type_inherits_its_params() {
        let types = vec![
            struct_with_field(
                "Leaf",
                "note",
                RustType::UnboundedString(param("LeafNoteCap", 1024)),
            ),
            struct_with_field("Outer", "leaf", RustType::Local("Leaf".to_string())),
        ];

        let params = compute_const_params(&types);

        assert_eq!(params["Outer"], vec![param("LeafNoteCap", 1024)]);
    }

    #[test]
    fn params_propagate_through_multiple_levels_of_nesting() {
        let types = vec![
            struct_with_field(
                "Leaf",
                "note",
                RustType::UnboundedString(param("LeafNoteCap", 1024)),
            ),
            struct_with_field("Middle", "leaf", RustType::Local("Leaf".to_string())),
            struct_with_field("Top", "middle", RustType::Local("Middle".to_string())),
        ];

        let params = compute_const_params(&types);

        assert_eq!(params["Top"], vec![param("LeafNoteCap", 1024)]);
    }

    #[test]
    fn params_propagate_through_a_bounded_vec_of_a_generic_local_type() {
        let types = vec![
            struct_with_field(
                "Leaf",
                "note",
                RustType::UnboundedString(param("LeafNoteCap", 1024)),
            ),
            struct_with_field(
                "Outer",
                "leaves",
                RustType::Vec(Box::new(RustType::Local("Leaf".to_string())), 4),
            ),
        ];

        let params = compute_const_params(&types);

        assert_eq!(params["Outer"], vec![param("LeafNoteCap", 1024)]);
    }

    #[test]
    fn struct_with_two_distinct_unbounded_fields_gets_both_params_in_field_order() {
        let types = vec![GeneratedType::Struct(RustStruct {
            name: "TwoFields".to_string(),
            action: None,
            description: None,
            fields: vec![
                RustField {
                    name: "a".to_string(),
                    ty: RustType::UnboundedString(param("TwoFieldsACap", 1024)),
                    optional: false,
                    description: None,
                },
                RustField {
                    name: "b".to_string(),
                    ty: RustType::UnboundedString(param("TwoFieldsBCap", 1024)),
                    optional: false,
                    description: None,
                },
            ],
        })];

        let params = compute_const_params(&types);

        assert_eq!(
            params["TwoFields"],
            vec![param("TwoFieldsACap", 1024), param("TwoFieldsBCap", 1024)]
        );
    }

    fn type_param(name: &str) -> TypeParam {
        TypeParam {
            name: name.to_string(),
            default: "()".to_string(),
        }
    }

    #[test]
    fn struct_with_an_untyped_field_gets_its_own_type_param() {
        let types = vec![struct_with_field(
            "DataTransferRequest",
            "data",
            RustType::Any(type_param("DataTransferRequestData")),
        )];

        let params = compute_type_params(&types);

        assert_eq!(
            params["DataTransferRequest"],
            vec![type_param("DataTransferRequestData")]
        );
    }

    #[test]
    fn type_params_propagate_through_local_references_and_vecs() {
        let types = vec![
            struct_with_field("Leaf", "data", RustType::Any(type_param("LeafData"))),
            struct_with_field("Middle", "leaf", RustType::Local("Leaf".to_string())),
            struct_with_field(
                "Top",
                "middles",
                RustType::Vec(Box::new(RustType::Local("Middle".to_string())), 4),
            ),
        ];

        let params = compute_type_params(&types);

        assert_eq!(params["Top"], vec![type_param("LeafData")]);
    }

    #[test]
    fn struct_with_no_untyped_field_gets_no_type_params() {
        let types = vec![struct_with_field("Plain", "value", RustType::Integer)];

        let params = compute_type_params(&types);

        assert!(params["Plain"].is_empty());
    }

    #[test]
    fn enums_never_get_params() {
        use crate::model::RustEnum;

        let types = vec![GeneratedType::Enum(RustEnum {
            name: "SomeEnum".to_string(),
            description: None,
            variants: Vec::new(),
        })];

        let params = compute_const_params(&types);

        assert!(params["SomeEnum"].is_empty());
    }
}
