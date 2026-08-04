use crate::model::GeneratedType;
use crate::model::RustField;
use crate::model::RustType;
use std::collections::HashMap;

/// Computes, for every named type in `types`, whether it can derive `Eq`
/// (not just `PartialEq`) -- transitively. A struct disqualifies itself by
/// having an `f64` field directly, or by referencing (through `Local`,
/// possibly through a `Vec`) another type that's disqualified. Enums, as
/// modeled here, never carry data and are always `Eq`.
///
/// This has to look at the whole type graph at once: a struct's own field
/// list doesn't say whether a nested struct three levels down secretly
/// contains a float, so checking each type in isolation (as the generator
/// used to) silently produced `derive(Eq)` on types that don't actually
/// implement it.
pub fn compute_eq_derivable(types: &[GeneratedType]) -> HashMap<String, bool> {
    let mut eq_by_name: HashMap<String, bool> = types
        .iter()
        .map(|ty| match ty {
            GeneratedType::Struct(s) => (s.name.clone(), true),
            GeneratedType::Enum(e) => (e.name.clone(), true),
        })
        .collect();

    // Monotonically decreasing (true -> false, never back), so this
    // converges in at most `types.len()` passes.
    loop {
        let mut changed = false;

        for ty in types {
            if let GeneratedType::Struct(s) = ty {
                let derivable = fields_are_eq(&s.fields, &eq_by_name);
                if eq_by_name.get(&s.name) != Some(&derivable) {
                    eq_by_name.insert(s.name.clone(), derivable);
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    eq_by_name
}

pub fn fields_are_eq(fields: &[RustField], eq_by_name: &HashMap<String, bool>) -> bool {
    fields.iter().all(|f| type_is_eq(&f.ty, eq_by_name))
}

fn type_is_eq(ty: &RustType, eq_by_name: &HashMap<String, bool>) -> bool {
    match ty {
        RustType::Number => false,
        RustType::Vec(inner, _) => type_is_eq(inner, eq_by_name),
        RustType::UnboundedVec(inner, _) => type_is_eq(inner, eq_by_name),
        // A string is `Eq` whether it's `heapless::String<N>` or
        // `alloc::string::String`.
        RustType::UnboundedString(_) => true,
        // Names not in the map are types outside this pool (currently only
        // hand-written `ocpp-types` primitives like `IdTag`), which are Eq.
        RustType::Local(name) => *eq_by_name.get(name).unwrap_or(&true),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RustStruct;

    fn struct_with_field(name: &str, ty: RustType) -> GeneratedType {
        GeneratedType::Struct(RustStruct {
            name: name.to_string(),
            action: None,
            description: None,
            fields: vec![RustField {
                name: "field".to_string(),
                ty,
                optional: false,
                description: None,
            }],
        })
    }

    #[test]
    fn struct_with_a_float_field_is_not_eq() {
        let types = vec![struct_with_field("Sample", RustType::Number)];

        let eq = compute_eq_derivable(&types);

        assert!(!eq["Sample"]);
    }

    #[test]
    fn struct_referencing_a_non_eq_type_transitively_is_not_eq() {
        let types = vec![
            struct_with_field("Inner", RustType::Number),
            struct_with_field("Outer", RustType::Local("Inner".to_string())),
        ];

        let eq = compute_eq_derivable(&types);

        assert!(!eq["Inner"]);
        assert!(!eq["Outer"]);
    }

    #[test]
    fn struct_referencing_a_non_eq_type_through_a_vec_is_not_eq() {
        let types = vec![
            struct_with_field("Inner", RustType::Number),
            struct_with_field(
                "Outer",
                RustType::Vec(Box::new(RustType::Local("Inner".to_string())), 3),
            ),
        ];

        let eq = compute_eq_derivable(&types);

        assert!(!eq["Outer"]);
    }

    #[test]
    fn struct_with_only_eq_fields_is_eq() {
        let types = vec![struct_with_field("Plain", RustType::Integer)];

        let eq = compute_eq_derivable(&types);

        assert!(eq["Plain"]);
    }

    #[test]
    fn deeply_nested_non_eq_type_propagates_up_multiple_levels() {
        let types = vec![
            struct_with_field("Level0", RustType::Number),
            struct_with_field("Level1", RustType::Local("Level0".to_string())),
            struct_with_field("Level2", RustType::Local("Level1".to_string())),
        ];

        let eq = compute_eq_derivable(&types);

        assert!(!eq["Level2"]);
    }
}
