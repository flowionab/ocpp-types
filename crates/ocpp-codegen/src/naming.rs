pub fn rust_name(name: &str) -> String {
    let mut result = String::new();

    for c in name.chars() {
        if c.is_uppercase() {
            if !result.is_empty() && !result.ends_with('_') {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }

    result
}

/// OCPP field names like `type` collide with Rust keywords. Callers
/// constructing the identifier token should use a raw identifier
/// (`Ident::new_raw`) when this returns true, rather than baking `r#` into
/// the name string (which `Ident::new` rejects outright).
pub fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
    )
}

/// Sanitizes an OCPP enum value (e.g. `Voltage.Maximum`, `L2-N`) into a
/// valid Rust identifier by dropping non-identifier characters, so callers
/// can compare the result against the original to decide whether a
/// `#[serde(rename = "...")]` is needed to preserve the wire value.
pub fn rust_variant_ident(raw: &str) -> String {
    let mut result: String = raw.chars().filter(|c| c.is_alphanumeric()).collect();

    if result.is_empty() {
        result.push('_');
    }

    // Rust variant naming convention is PascalCase; some OCPP enum values
    // (e.g. `wInductive`) start lowercase, which is a valid identifier but
    // trips `non_camel_case_types`.
    if let Some(first) = result.chars().next().filter(|c| c.is_lowercase()) {
        result.replace_range(0..first.len_utf8(), &first.to_uppercase().to_string());
    }

    if result.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        result.insert(0, '_');
    }

    result
}

/// Capitalizes only the first character, e.g. `idTagInfo` -> `IdTagInfo`.
/// Used to synthesize a Rust type name for an inline (`$ref`-less) nested
/// object/enum schema from its field name -- since it's keyed purely on
/// the field name (not the owning struct too), two fields with the same
/// name and shape in different messages naturally end up sharing one
/// generated type instead of getting a duplicate each.
pub fn pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// The const generic parameter name for a field with no spec-given bound,
/// e.g. `ChargingSchedule` + `chargingSchedulePeriod` ->
/// `CHARGING_SCHEDULE_CHARGING_SCHEDULE_PERIOD_CAP`. Deliberately includes
/// both the owning struct and field name (not just the field) so it stays
/// unique when several unbounded fields end up threaded onto the same
/// ancestor struct (see `crate::generics`).
pub fn const_param_name(owner: &str, field_name: &str) -> String {
    // Const generic parameters follow the same `non_upper_case_globals`
    // convention as `const` items (SCREAMING_SNAKE_CASE), unlike type
    // parameters -- `rust_name` already does camelCase/PascalCase ->
    // snake_case for exactly this kind of identifier.
    format!(
        "{}_{}_CAP",
        rust_name(owner).to_uppercase(),
        rust_name(field_name).to_uppercase()
    )
}

/// The generic *type* parameter name for a schema property that declares no
/// type at all (2.0.1/2.1's `DataTransfer.data`, "open to implementation"),
/// e.g. `DataTransferRequest` + `data` -> `DataTransferRequestData`. Owner-
/// qualified for the same reason as [`const_param_name`]: several such
/// fields can end up threaded onto one ancestor struct.
pub fn type_param_name(owner: &str, field_name: &str) -> String {
    // Type parameters are PascalCase (`non_camel_case_types`), unlike the
    // SCREAMING_SNAKE_CASE const params.
    format!("{}{}", pascal_case(owner), pascal_case(field_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_prefix_a_leading_underscore_for_pascal_case_input() {
        assert_eq!(rust_name("BootNotificationRequest"), "boot_notification_request");
    }

    #[test]
    fn does_not_double_up_an_underscore_already_before_an_uppercase_letter() {
        // Real OCPP 2.1 field name: `limit_L2`.
        assert_eq!(rust_name("limit_L2"), "limit_l2");
    }

    #[test]
    fn identifies_rust_keywords() {
        assert!(is_rust_keyword("type"));
        assert!(!is_rust_keyword("reason"));
    }

    #[test]
    fn leaves_plain_pascal_case_untouched() {
        assert_eq!(rust_variant_ident("PowerUp"), "PowerUp");
    }

    #[test]
    fn pascal_case_capitalizes_only_the_first_letter() {
        assert_eq!(pascal_case("idTagInfo"), "IdTagInfo");
    }

    #[test]
    fn strips_dots_and_dashes() {
        assert_eq!(rust_variant_ident("Voltage.Maximum"), "VoltageMaximum");
        assert_eq!(rust_variant_ident("L2-N"), "L2N");
    }

    #[test]
    fn prefixes_leading_digits() {
        assert_eq!(rust_variant_ident("3Phase"), "_3Phase");
    }

    #[test]
    fn upper_cases_a_lowercase_leading_letter() {
        // Real OCPP 2.0.1 `EnergyTransferModeEnumType` values, e.g.
        // `wInductive` -- valid identifiers already, but not
        // `non_camel_case_types`-clean without this.
        assert_eq!(rust_variant_ident("wInductive"), "WInductive");
        assert_eq!(rust_variant_ident("wResonant"), "WResonant");
    }

    #[test]
    fn type_param_name_combines_owner_and_field_in_pascal_case() {
        assert_eq!(type_param_name("DataTransferRequest", "data"), "DataTransferRequestData");
    }

    #[test]
    fn const_param_name_combines_owner_and_field() {
        assert_eq!(
            const_param_name("ChargingSchedule", "chargingSchedulePeriod"),
            "CHARGING_SCHEDULE_CHARGING_SCHEDULE_PERIOD_CAP"
        );
    }
}
