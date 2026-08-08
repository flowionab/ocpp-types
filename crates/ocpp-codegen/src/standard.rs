//! Generates the `standard` module for a protocol version: the value sets
//! the specification defines for fields its JSON schemas type as bare
//! strings.
//!
//! A field like `SecurityEventNotificationRequest.type` or `Variable.name`
//! is `{"type": "string", "maxLength": 50}` in the schema -- the schema
//! never lists the values, so a schema-only generator cannot know them.
//! The spec states them in appendix tables, which this repo vendors as CSV
//! under `csv/<version>/` (see [`crate::csv`]).
//!
//! These generate *alongside* the wire types rather than replacing them.
//! OCPP explicitly permits vendor-specific components, variables,
//! configuration keys and security events, so narrowing `Variable.name`
//! from a string to an enum would make the crate reject conformant
//! traffic. The wire types keep their `heapless::String` fields; this
//! module adds the standardized values as enums with `as_str`/`from_wire`,
//! which is purely additive.

use quote::quote;

use crate::model::OcppVersion;

/// How a metadata column is exposed on the generated enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    /// `Option<&'static str>` -- the raw cell, `None` when blank.
    Text,
    /// `bool`, from a column whose only values are `Yes`/`No`.
    YesNo,
}

/// A column carried onto the generated enum as an accessor method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataColumn {
    /// CSV header to read.
    pub column: String,
    /// Generated method name.
    pub accessor: String,
    pub kind: MetadataKind,
    pub doc: String,
}

/// How a table's rows map to values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowLayout {
    /// One value per row, read from the value column.
    Flat,
    /// Rows are interleaved section headings and values: a row with an
    /// empty value column names the group that following rows belong to.
    /// `reason_codes.csv` is laid out this way.
    Grouped {
        /// Column holding the group name on heading rows.
        group_column: &'static str,
    },
}

/// One generated enum: which CSV files feed it and how to read them.
#[derive(Debug, Clone)]
pub struct ValueSetSpec {
    /// Generated enum name.
    pub name: &'static str,
    /// CSV file names within the version's directory. More than one when
    /// the spec splits a single wire field's values across tables.
    pub files: &'static [&'static str],
    /// Column holding the wire value.
    pub value_column: &'static str,
    /// Column holding the per-value description, rendered as a doc comment.
    pub doc_column: Option<&'static str>,
    pub layout: RowLayout,
    pub metadata: &'static [MetadataColumnSpec],
    /// Doc comment for the enum itself. Should say which wire field the
    /// values belong to, since that's not otherwise discoverable.
    pub doc: &'static str,
    /// When set, the enum gets an `Other(heapless::String<N>)` variant so a
    /// vendor-specific value round-trips instead of failing to deserialize.
    /// `N` is the field's spec `maxLength` -- the same bound the wire field
    /// carries, so `Other` can hold anything the field can.
    ///
    /// Use this for the value sets a deployment realistically extends:
    /// components, variables and configuration keys are all explicitly
    /// extensible in OCPP. It is not free -- see [`generate_value_set`] for
    /// what an open enum gives up.
    pub other_capacity: Option<usize>,
}

/// Static half of [`MetadataColumn`], for the registry below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataColumnSpec {
    pub column: &'static str,
    pub accessor: &'static str,
    pub kind: MetadataKind,
    pub doc: &'static str,
}

const COMPONENTS: ValueSetSpec = ValueSetSpec {
    name: "ComponentName",
    files: &["components.csv"],
    value_column: "Component",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `Component.name` values.\n\nThe schema types `Component.name` as a plain string, since a Charging \
          Station may expose vendor-specific components alongside these.",
    other_capacity: Some(50),
};

const VARIABLES: ValueSetSpec = ValueSetSpec {
    name: "VariableName",
    files: &["variables.csv"],
    value_column: "Name",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[
        MetadataColumnSpec {
            column: "DataType",
            accessor: "data_type",
            kind: MetadataKind::Text,
            doc: "The spec's data type for this variable, as named in the device model tables (e.g. `decimal`, `OptionList`).",
        },
        MetadataColumnSpec {
            column: "Unit",
            accessor: "unit",
            kind: MetadataKind::Text,
            doc: "The variable's unit, where the spec states one.",
        },
    ],
    doc: "Standardized `Variable.name` values.\n\nThe schema types `Variable.name` as a plain string, since a Charging \
          Station may expose vendor-specific variables alongside these.",
    other_capacity: Some(50),
};

const SECURITY_EVENTS: ValueSetSpec = ValueSetSpec {
    name: "SecurityEvent",
    files: &["security_events.csv"],
    value_column: "Security Event",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[MetadataColumnSpec {
        column: "Critical",
        accessor: "is_critical",
        kind: MetadataKind::YesNo,
        doc: "Whether the spec marks this event critical, meaning it must be reported to the CSMS even when not \
              explicitly monitored.",
    }],
    doc: "Standardized `SecurityEventNotificationRequest.type` values.",
    other_capacity: None,
};

const REASON_CODES: ValueSetSpec = ValueSetSpec {
    name: "ReasonCode",
    files: &["reason_codes.csv"],
    value_column: "Reason code",
    doc_column: Some("Description"),
    layout: RowLayout::Grouped { group_column: "Group" },
    metadata: &[MetadataColumnSpec {
        column: "Typically used for",
        accessor: "typically_used_for",
        kind: MetadataKind::Text,
        doc: "The message(s) the spec expects this reason code on.",
    }],
    doc: "Standardized `StatusInfo.reasonCode` values.",
    other_capacity: None,
};

const UNITS: ValueSetSpec = ValueSetSpec {
    name: "Unit",
    files: &["units_of_measure.csv"],
    value_column: "Value",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `UnitOfMeasure.unit` values.\n\nNamed `Unit` rather than `UnitOfMeasure` to leave that name to \
          the schema-generated struct of the same name in `common`.",
    other_capacity: None,
};

const CONNECTOR_TYPES: ValueSetSpec = ValueSetSpec {
    name: "ConnectorType",
    files: &["connectorenumtype.csv"],
    value_column: "Value",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `Connector` component `ConnectorType` values.",
    other_capacity: None,
};

const ID_TOKEN_TYPES: ValueSetSpec = ValueSetSpec {
    name: "IdTokenType",
    files: &["idtokenenumtype.csv"],
    value_column: "Value",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `IdToken.type` values.\n\n2.1 widened this field from an enum to a string, so the values are \
          only listed in the spec's appendix rather than in the schema.",
    other_capacity: None,
};

const CHARGING_LIMIT_SOURCES: ValueSetSpec = ValueSetSpec {
    name: "ChargingLimitSource",
    files: &["charginglimitsourceenumtype.csv"],
    value_column: "Value",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `chargingLimitSource` values.\n\n2.1 widened this field from an enum to a string, so the values \
          are only listed in the spec's appendix rather than in the schema.",
    other_capacity: None,
};

const PAYMENT_BRANDS: ValueSetSpec = ValueSetSpec {
    name: "PaymentBrand",
    files: &["paymentbrand.csv"],
    value_column: "PaymentBrand",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `paymentBrand` values, for the ad hoc payment flow.",
    other_capacity: None,
};

const PAYMENT_RECOGNITIONS: ValueSetSpec = ValueSetSpec {
    name: "PaymentRecognition",
    files: &["paymentrecognition.csv"],
    value_column: "PaymentRecognition",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `paymentRecognition` values, for the ad hoc payment flow.",
    other_capacity: None,
};

const SIGNING_METHODS: ValueSetSpec = ValueSetSpec {
    name: "SigningMethod",
    files: &["signingmethod.csv"],
    value_column: "SigningMethod",
    doc_column: None,
    layout: RowLayout::Flat,
    metadata: &[
        MetadataColumnSpec {
            column: "Algorithm",
            accessor: "algorithm",
            kind: MetadataKind::Text,
            doc: "The signature algorithm.",
        },
        MetadataColumnSpec {
            column: "Curve",
            accessor: "curve",
            kind: MetadataKind::Text,
            doc: "The elliptic curve.",
        },
        MetadataColumnSpec {
            column: "Key Length",
            accessor: "key_length",
            kind: MetadataKind::Text,
            doc: "The key length, as the spec states it.",
        },
        MetadataColumnSpec {
            column: "Hash Algorithm",
            accessor: "hash_algorithm",
            kind: MetadataKind::Text,
            doc: "The hash algorithm paired with the signature.",
        },
    ],
    doc: "Standardized `signingMethod` values, for ISO 15118 price schedule signatures.",
    other_capacity: None,
};

const ADDITIONAL_INFO_TYPES: ValueSetSpec = ValueSetSpec {
    name: "AdditionalInfoType",
    files: &["additional_info_types.csv", "additional_info_types_adhoc.csv"],
    value_column: "additionalInfo.type",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[],
    doc: "Standardized `AdditionalInfo.type` values.\n\nMerged from the spec's general table and its ad hoc payment \
          table, which both populate the same wire field.",
    other_capacity: None,
};

/// The value sets to generate for a version, in output order. A spec whose
/// files are all absent is skipped, so a version directory need not carry
/// every table.
pub fn specs_for(version: OcppVersion) -> &'static [ValueSetSpec] {
    match version {
        // 1.6J's value sets are authored from the spec text rather than an
        // export; see `csv/ocpp1.6j/README.md`. No measurand table: the 1.6
        // schemas already enumerate `Measurand`, `Phase` and
        // `SampledValueItemUnit`, so those come from `common` as usual.
        OcppVersion::V16 => &[CONFIGURATION_KEYS, SECURITY_EVENTS_16],
        OcppVersion::V201 => &[COMPONENTS, VARIABLES, SECURITY_EVENTS, REASON_CODES, UNITS],
        OcppVersion::V21 => &[
            COMPONENTS,
            VARIABLES,
            SECURITY_EVENTS,
            REASON_CODES,
            UNITS,
            CONNECTOR_TYPES,
            ID_TOKEN_TYPES,
            CHARGING_LIMIT_SOURCES,
            PAYMENT_BRANDS,
            PAYMENT_RECOGNITIONS,
            SIGNING_METHODS,
            ADDITIONAL_INFO_TYPES,
        ],
    }
}

const CONFIGURATION_KEYS: ValueSetSpec = ValueSetSpec {
    name: "ConfigurationKey",
    files: &["configuration_keys.csv"],
    value_column: "Key",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[
        MetadataColumnSpec {
            column: "Accessibility",
            accessor: "accessibility",
            kind: MetadataKind::Text,
            doc: "`R` (read-only), `RW` (read/write), or `W`, as stated by the spec.",
        },
        MetadataColumnSpec {
            column: "Type",
            accessor: "value_type",
            kind: MetadataKind::Text,
            doc: "The value's type as stated by the spec (e.g. `integer`, `boolean`, `CSL`).",
        },
        MetadataColumnSpec {
            column: "Required",
            accessor: "is_required",
            kind: MetadataKind::YesNo,
            doc: "Whether the spec requires every Charging Station to support this key.",
        },
        MetadataColumnSpec {
            column: "Feature Profile",
            accessor: "feature_profile",
            kind: MetadataKind::Text,
            doc: "The feature profile that defines the key (e.g. `Core`, `SmartCharging`, `Security`).",
        },
    ],
    doc: "Standardized configuration keys for `GetConfiguration` / `ChangeConfiguration`.\n\nThe schema types `key` as \
          a plain string; the spec lists these in \"Standard Configuration Key Names & Values\", extended by the \
          Security Whitepaper.",
    other_capacity: Some(50),
};

const SECURITY_EVENTS_16: ValueSetSpec = ValueSetSpec {
    name: "SecurityEvent",
    files: &["security_events.csv"],
    value_column: "Security Event",
    doc_column: Some("Description"),
    layout: RowLayout::Flat,
    metadata: &[MetadataColumnSpec {
        column: "Critical",
        accessor: "is_critical",
        kind: MetadataKind::YesNo,
        doc: "Whether the Security Whitepaper marks this event critical, meaning it must be reported to the Central \
              System even when not explicitly monitored.",
    }],
    doc: "Standardized `SecurityEventNotification.type` values, from the 1.6 Security Whitepaper.",
    other_capacity: None,
};

/// A value set resolved against real CSV data, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSet {
    pub name: String,
    pub doc: String,
    pub values: Vec<ValueEntry>,
    pub metadata: Vec<MetadataColumn>,
    /// See [`ValueSetSpec::other_capacity`].
    pub other_capacity: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueEntry {
    /// Sanitized Rust variant identifier.
    pub ident: String,
    /// The value as it appears on the wire.
    pub raw: String,
    pub doc: Option<String>,
    /// One entry per [`ValueSet::metadata`] column, in the same order.
    pub metadata: Vec<Option<String>>,
    /// Group name, for a [`RowLayout::Grouped`] table.
    pub group: Option<String>,
}

/// Collapses each run of whitespace in a description to a single space.
///
/// These cells come out of a spreadsheet and carry its formatting: the
/// 2.x `Model` variable's description has a run of four literal tabs mid
/// sentence. Passed through, they land in a `///` comment, where they mean
/// nothing to a reader and trip `clippy::tabs_in_doc_comments` in the
/// published crate.
fn normalize_description(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");

    // These cells are prose, but they land in `///` comments, which rustdoc
    // parses as CommonMark. The device model uses angle brackets for
    // placeholders (`"<uri>,<major>,<minor>"`) and square brackets for
    // citations (`[RFC3339]`), which rustdoc would otherwise read as an
    // unclosed HTML tag and a broken intra-doc link -- 38 warnings, and
    // mangled text on docs.rs. Backslash escapes render as the original
    // character.
    let mut escaped = String::with_capacity(collapsed.len());

    for character in collapsed.chars() {
        if matches!(character, '<' | '>' | '[' | ']') {
            escaped.push('\\');
        }

        escaped.push(character);
    }

    escaped
}

/// Resolves a spec against its CSV tables. `read` supplies a file's
/// contents, or `None` if that file doesn't exist for this version.
pub fn resolve(
    spec: &ValueSetSpec,
    mut read: impl FnMut(&str) -> Option<String>,
) -> Option<ValueSet> {
    let mut values: Vec<ValueEntry> = Vec::new();
    let mut any_file = false;

    for file in spec.files {
        let Some(content) = read(file) else { continue };
        any_file = true;

        let table = crate::csv::parse(&content);
        let mut group: Option<String> = None;

        for row in &table.rows {
            let raw = table.get(row, spec.value_column);

            // A grouped table interleaves section headings with values; a
            // heading has no value, and names the group for what follows.
            let Some(raw) = raw else {
                if let RowLayout::Grouped { group_column } = spec.layout
                    && let Some(heading) = table.get(row, group_column)
                {
                    group = Some(heading.to_string());
                }
                continue;
            };

            let doc = spec
                .doc_column
                .and_then(|c| table.get(row, c))
                .map(normalize_description);
            let metadata = spec
                .metadata
                .iter()
                .map(|m| table.get(row, m.column).map(str::to_string))
                .collect();

            // The same wire value can be listed twice with different
            // metadata (2.x `variables.csv` lists `Timeout` as both a
            // generic decimal and a BatterySwapCtrlr integer). One wire
            // value is one variant, so keep the first and fold the second
            // description in rather than dropping it silently.
            if let Some(existing) = values.iter_mut().find(|v| v.raw == raw) {
                if let Some(extra) = doc {
                    match &mut existing.doc {
                        Some(text) if !text.contains(&extra) => {
                            text.push_str("\n\nAlso listed as: ");
                            text.push_str(&extra);
                        }
                        Some(_) => {}
                        slot @ None => *slot = Some(extra),
                    }
                }
                continue;
            }

            values.push(ValueEntry {
                ident: crate::naming::rust_variant_ident(raw),
                raw: raw.to_string(),
                doc,
                metadata,
                group: group.clone(),
            });
        }
    }

    if !any_file || values.is_empty() {
        return None;
    }

    Some(ValueSet {
        name: spec.name.to_string(),
        doc: spec.doc.to_string(),
        other_capacity: spec.other_capacity,
        values,
        metadata: spec
            .metadata
            .iter()
            .map(|m| MetadataColumn {
                column: m.column.to_string(),
                accessor: m.accessor.to_string(),
                kind: m.kind,
                doc: m.doc.to_string(),
            })
            .collect(),
    })
}

/// One `(component, variable)` pair of the 2.x standardized device model,
/// from `dm_components_vars.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceModelRow {
    /// `None` for the table's `<generic>` marker, meaning the variable
    /// applies to any component rather than a named one.
    pub component: Option<String>,
    pub variable: String,
    /// The `VariableAttribute.type` values this row is about, when the
    /// table used its `Variable(Attr)` notation -- e.g. `Min/MaxSet`.
    pub attributes: Option<String>,
    pub instance: Option<String>,
    pub required: Option<String>,
    pub data_type: Option<String>,
    pub unit: Option<String>,
}

/// Marker the device-model table uses for a variable that isn't tied to one
/// component.
const GENERIC_COMPONENT: &str = "<generic>";

/// Splits the device-model table's attribute notation off a variable name:
/// `ACCurrent(Min/MaxSet)` -> `("ACCurrent", Some("Min/MaxSet"))`.
///
/// The parenthetical is not part of the name. It names which
/// `VariableAttribute.type` values the row is about (`MinSet`, `MaxSet`,
/// `Target` are `AttributeEnumType` values), and the same variable appears
/// bare elsewhere in the table -- `ACCurrent` is listed under `<generic>`.
/// Taking the parenthesized form as a wire name would produce values no
/// conformant station ever sends.
pub fn split_attribute_notation(name: &str) -> (&str, Option<&str>) {
    match name.split_once('(') {
        Some((base, rest)) => (
            base.trim_end(),
            Some(rest.trim_end_matches(')').trim()).filter(|text| !text.is_empty()),
        ),
        None => (name, None),
    }
}

pub fn resolve_device_model(content: &str) -> Vec<DeviceModelRow> {
    let table = crate::csv::parse(content);

    table
        .rows
        .iter()
        .filter_map(|row| {
            let (variable, attributes) = split_attribute_notation(table.get(row, "Variable")?);
            let component = table
                .get(row, "Specific Component")
                .filter(|name| *name != GENERIC_COMPONENT)
                .map(str::to_string);

            Some(DeviceModelRow {
                component,
                variable: variable.to_string(),
                attributes: attributes.map(str::to_string),
                instance: table.get(row, "Instance").map(str::to_string),
                required: table.get(row, "Required?").map(str::to_string),
                data_type: table.get(row, "DataType").map(str::to_string),
                unit: table.get(row, "Unit").map(str::to_string),
            })
        })
        .collect()
}

/// Which device-model column supplies extra names for a value set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceModelNames {
    Component,
    Variable,
}

/// Folds names from the device-model table into `set`, returning those that
/// weren't already there.
///
/// The dedicated `components.csv`/`variables.csv` tables are not supersets
/// of the device-model table: 2.x `variables.csv` omits around a hundred
/// names the device model lists (`Apn`, `OcppCsmsUrl`, `VpnServer`,
/// `WebSocketPingInterval`, ...), all of them real standardized variables.
/// Generating from the dedicated table alone leaves the enum missing
/// exactly the configuration-oriented names callers reach for most.
///
/// The returned list is worth reporting rather than discarding: a name that
/// appears *only* in the device-model table is either a genuine addition or
/// a typo in the export (`TxCtlrl` for `TxCtrlr`), and the two are only
/// distinguishable by a human.
pub fn merge_device_model_names(
    set: &mut ValueSet,
    rows: &[DeviceModelRow],
    source: DeviceModelNames,
) -> Vec<String> {
    let mut added = Vec::new();

    for row in rows {
        let name = match source {
            DeviceModelNames::Component => match &row.component {
                Some(component) => component.as_str(),
                None => continue,
            },
            DeviceModelNames::Variable => row.variable.as_str(),
        };

        if set.values.iter().any(|value| value.raw == name) {
            continue;
        }

        set.values.push(ValueEntry {
            ident: crate::naming::rust_variant_ident(name),
            raw: name.to_string(),
            doc: Some(
                "Listed in the specification's device model table but not in its \
                 dedicated name table."
                    .to_string(),
            ),
            metadata: vec![None; set.metadata.len()],
            group: None,
        });
        added.push(name.to_string());
    }

    added
}

fn ident(name: &str) -> proc_macro2::Ident {
    proc_macro2::Ident::new(name, proc_macro2::Span::call_site())
}

/// Renders one value set: the enum, its `ALL`/`as_str`/`from_wire`
/// inherent methods, its metadata accessors, and `Display`/`FromStr`.
///
/// A set with [`ValueSet::other_capacity`] renders as an *open* enum, with an
/// `Other(heapless::String<N>)` variant so a vendor-specific value
/// deserializes instead of erroring. That costs three things, which is why it
/// isn't the default for every set:
///
/// - the enum is no longer `Copy`, and grows from one byte to the width of a
///   `heapless::String<N>` -- material on an MCU;
/// - `as_str` returns `&str` rather than `&'static str`;
/// - `serde` cannot derive it, so `Serialize`/`Deserialize` are hand-rolled
///   (serde has no way to express "unit variants plus a string fallback"
///   for an externally-tagged string enum).
///
/// `PartialEq`/`Eq`/`Hash`/`Ord` compare the wire string rather than the
/// variant, so `Other("HeartbeatInterval")` equals
/// `ConfigurationKey::HeartbeatInterval`. Deriving them instead would make
/// equality depend on how a value was constructed, which is a subtle way to
/// get a lookup miss.
pub fn generate_value_set(set: &ValueSet) -> proc_macro2::TokenStream {
    let name = ident(&set.name);
    let set_doc = crate::rust::doc_attrs(&Some(set.doc.clone()));
    let open = set.other_capacity.is_some();

    let variants = set.values.iter().map(|value| {
        let vident = ident(&value.ident);
        let doc = crate::rust::doc_attrs(&value.doc);
        let group_doc = match &value.group {
            Some(group) => {
                let line = format!(" Spec group: {group}.");
                quote! { #[doc = ""] #[doc = #line] }
            }
            None => quote! {},
        };

        // An open enum's serde impls are hand-rolled and map through
        // `as_str`/`from_wire_or_other`, so there is no derive to consume a
        // `serde(rename)` -- emitting one would be an unknown attribute.
        if value.ident == value.raw || open {
            quote! { #doc #group_doc #vident }
        } else {
            let raw = &value.raw;
            quote! {
                #doc
                #group_doc
                #[cfg_attr(feature = "serde", serde(rename = #raw))]
                #vident
            }
        }
    });

    let all = set.values.iter().map(|value| {
        let vident = ident(&value.ident);
        quote! { Self::#vident }
    });

    let as_str_arms = set.values.iter().map(|value| {
        let vident = ident(&value.ident);
        let raw = &value.raw;
        quote! { Self::#vident => #raw }
    });

    let from_wire_arms = set.values.iter().map(|value| {
        let vident = ident(&value.ident);
        let raw = &value.raw;
        quote! { #raw => Some(Self::#vident) }
    });

    let accessors = set.metadata.iter().enumerate().map(|(index, column)| {
        let method = ident(&column.accessor);
        let doc = crate::rust::doc_attrs(&Some(column.doc.clone()));

        match column.kind {
            MetadataKind::Text => {
                let arms = set.values.iter().map(|value| {
                    let vident = ident(&value.ident);
                    match value.metadata.get(index).and_then(Option::as_deref) {
                        Some(text) => quote! { Self::#vident => Some(#text) },
                        None => quote! { Self::#vident => None },
                    }
                });

                // An open enum's `Other` has no spec metadata, and binding a
                // `heapless::String` in a `const fn` match isn't allowed, so
                // these drop `const` there.
                let other_arm = open.then(|| quote! { Self::Other(_) => None });
                let constness = (!open).then(|| quote! { const });

                quote! {
                    #doc
                    pub #constness fn #method(&self) -> Option<&'static str> {
                        match self { #(#arms,)* #other_arm }
                    }
                }
            }
            MetadataKind::YesNo => {
                let arms = set.values.iter().map(|value| {
                    let vident = ident(&value.ident);
                    let yes = value
                        .metadata
                        .get(index)
                        .and_then(Option::as_deref)
                        .is_some_and(|text| text.eq_ignore_ascii_case("yes"));
                    quote! { Self::#vident => #yes }
                });

                let other_arm = open.then(|| quote! { Self::Other(_) => false });
                let constness = (!open).then(|| quote! { const });

                quote! {
                    #doc
                    pub #constness fn #method(&self) -> bool {
                        match self { #(#arms,)* #other_arm }
                    }
                }
            }
        }
    });

    let count = set.values.len();
    let all_doc = format!(
        " Every value this version's specification defines ({count}), in spec order."
    );

    let Some(capacity) = set.other_capacity else {
        return quote! {
            #set_doc
            #[doc = ""]
            #[doc = " A *closed* set: it holds only the values the specification"]
            #[doc = " defines. The wire field is a string, so a deployment can still"]
            #[doc = " send something else -- `from_wire` returns `None` for that,"]
            #[doc = " which is not by itself a protocol error."]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            pub enum #name {
                #(#variants,)*
            }

            impl #name {
                #[doc = #all_doc]
                pub const ALL: &'static [Self] = &[#(#all,)*];

                /// This value as it appears on the wire.
                pub const fn as_str(&self) -> &'static str {
                    match self { #(#as_str_arms,)* }
                }

                /// Parses a wire value, returning `None` for values the
                /// specification doesn't define (e.g. a vendor's own).
                pub fn from_wire(value: &str) -> Option<Self> {
                    match value {
                        #(#from_wire_arms,)*
                        _ => None,
                    }
                }

                #(#accessors)*
            }

            impl core::fmt::Display for #name {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.write_str(self.as_str())
                }
            }

            impl core::str::FromStr for #name {
                type Err = UnknownValue;

                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    Self::from_wire(value).ok_or(UnknownValue)
                }
            }
        };
    };

    let expecting = format!("a {} string", set.name);
    let other_doc = format!(
        " A value this version's specification doesn't define -- typically a\n \
         vendor-specific one, which OCPP explicitly permits.\n\n \
         Bounded at {capacity} bytes, the same `maxLength` the wire field carries,\n \
         so anything the field can hold this can hold. Prefer\n \
         [`Self::from_wire_or_other`] over constructing this directly: it\n \
         returns the standardized variant when the value is one, keeping a\n \
         single representation per wire string."
    );

    quote! {
        #set_doc
        #[doc = ""]
        #[doc = " An *open* set: values the specification doesn't define are carried"]
        #[doc = " in [`Self::Other`] rather than rejected, so a deployment's own"]
        #[doc = " values survive a deserialize/serialize round trip."]
        #[doc = ""]
        #[doc = " Comparison and hashing go through the wire string, not the variant,"]
        #[doc = " so `Other(\"...\")` holding a standardized value compares equal to"]
        #[doc = " that variant."]
        #[derive(Debug, Clone)]
        pub enum #name {
            #(#variants,)*

            #[doc = #other_doc]
            Other(heapless::String<#capacity>),
        }

        impl #name {
            #[doc = #all_doc]
            #[doc = ""]
            #[doc = " Does not include [`Self::Other`], which is unbounded in the"]
            #[doc = " values it can hold."]
            pub const ALL: &'static [Self] = &[#(#all,)*];

            /// This value as it appears on the wire.
            pub fn as_str(&self) -> &str {
                match self {
                    #(#as_str_arms,)*
                    Self::Other(value) => value.as_str(),
                }
            }

            /// Parses a wire value, returning `None` for values the
            /// specification doesn't define.
            ///
            /// Use this to ask "is this one of the spec's values?". To accept
            /// any value, use [`Self::from_wire_or_other`].
            pub fn from_wire(value: &str) -> Option<Self> {
                match value {
                    #(#from_wire_arms,)*
                    _ => None,
                }
            }

            /// Parses any wire value, falling back to [`Self::Other`].
            ///
            /// Fails only if `value` is longer than the specification's
            /// `maxLength` for this field, in which case it isn't a value the
            /// field could have carried in the first place.
            pub fn from_wire_or_other(value: &str) -> Result<Self, ValueTooLong> {
                if let Some(standardized) = Self::from_wire(value) {
                    return Ok(standardized);
                }

                heapless::String::try_from(value)
                    .map(Self::Other)
                    .map_err(|_| ValueTooLong)
            }

            /// Whether this is one of the values the specification defines,
            /// as opposed to a vendor's own.
            pub fn is_standardized(&self) -> bool {
                !matches!(self, Self::Other(_))
            }

            #(#accessors)*
        }

        impl core::fmt::Display for #name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl core::str::FromStr for #name {
            type Err = ValueTooLong;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::from_wire_or_other(value)
            }
        }

        // Compare and hash on the wire string so the two ways of spelling one
        // value agree: `Other("X")` and the `X` variant are the same value.
        impl PartialEq for #name {
            fn eq(&self, other: &Self) -> bool {
                self.as_str() == other.as_str()
            }
        }

        impl Eq for #name {}

        impl core::hash::Hash for #name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                self.as_str().hash(state);
            }
        }

        impl PartialOrd for #name {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for #name {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.as_str().cmp(other.as_str())
            }
        }

        // serde can't derive this: there is no attribute for "unit variants
        // plus a string fallback" on an externally-tagged enum, so the
        // fallback has to be hand-rolled through a visitor.
        #[cfg(feature = "serde")]
        impl serde::Serialize for #name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for #name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'v> serde::de::Visitor<'v> for Visitor {
                    type Value = #name;

                    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                        f.write_str(#expecting)
                    }

                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        #name::from_wire_or_other(value).map_err(serde::de::Error::custom)
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }
    }
}

/// Renders the device-model table as a `const` slice.
pub fn generate_device_model(rows: &[DeviceModelRow]) -> proc_macro2::TokenStream {
    let entries = rows.iter().map(|row| {
        let component = option_str(&row.component);
        let variable = &row.variable;
        let attributes = option_str(&row.attributes);
        let instance = option_str(&row.instance);
        let required = option_str(&row.required);
        let data_type = option_str(&row.data_type);
        let unit = option_str(&row.unit);

        quote! {
            DeviceModelEntry {
                component: #component,
                variable: #variable,
                attributes: #attributes,
                instance: #instance,
                required: #required,
                data_type: #data_type,
                unit: #unit,
            }
        }
    });

    let count = rows.len();
    let table_doc = format!(
        " Every `(component, variable)` pair the standardized device model defines ({count})."
    );

    quote! {
        /// One `(component, variable)` pair of the standardized device model.
        ///
        /// The schemas describe the *shape* of `GetVariables`/`SetVariables`
        /// but not which pairs are meaningful; the spec states those in its
        /// device model appendix.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct DeviceModelEntry {
            /// `None` when the variable applies to any component rather
            /// than a named one (the spec's `<generic>` rows).
            pub component: Option<&'static str>,
            pub variable: &'static str,
            /// The `VariableAttribute.type` values this row is about, where
            /// the spec's table narrowed them -- e.g. `Min/MaxSet` for a row
            /// describing only a variable's `MinSet` and `MaxSet`
            /// attributes. `None` means the row is about the variable
            /// generally.
            pub attributes: Option<&'static str>,
            /// The `Variable.instance` this row describes, where the spec
            /// names one.
            pub instance: Option<&'static str>,
            /// Whether support is required: `yes`, `no`, or `V2X` (required
            /// only for V2X-capable stations). Kept as the spec's own text
            /// rather than a bool, since it is not a two-state field.
            pub required: Option<&'static str>,
            pub data_type: Option<&'static str>,
            pub unit: Option<&'static str>,
        }

        #[doc = #table_doc]
        pub const DEVICE_MODEL: &[DeviceModelEntry] = &[#(#entries,)*];
    }
}

fn option_str(value: &Option<String>) -> proc_macro2::TokenStream {
    match value {
        Some(text) => quote! { Some(#text) },
        None => quote! { None },
    }
}

/// The error `FromStr` yields for a value outside a *closed* set.
pub fn generate_unknown_value() -> proc_macro2::TokenStream {
    quote! {
        /// Returned by `FromStr` for a value the specification doesn't
        /// define.
        ///
        /// Not in itself a protocol error: OCPP permits vendor-specific
        /// components, variables, configuration keys and security events,
        /// so receiving one of those is conformant. Use `from_wire` when an
        /// unrecognized value is expected and `Option` reads better than
        /// `Result`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct UnknownValue;

        impl core::fmt::Display for UnknownValue {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("not a value defined by this OCPP version's specification")
            }
        }

        impl core::error::Error for UnknownValue {}
    }
}

/// The error an *open* set's `from_wire_or_other` yields. Emitted only when
/// the version has at least one open set, so no version carries an error type
/// nothing can return.
pub fn generate_value_too_long() -> proc_macro2::TokenStream {
    quote! {
        /// Returned when a value is longer than the specification's
        /// `maxLength` for its field.
        ///
        /// Only an open value set can produce this, and only from
        /// `from_wire_or_other`: a value too long for the field is not one the
        /// field could have carried, so it is rejected rather than truncated.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct ValueTooLong;

        impl core::fmt::Display for ValueTooLong {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("longer than this field's maxLength in the specification")
            }
        }

        impl core::error::Error for ValueTooLong {}
    }
}

/// Assembles a version's whole `standard` module.
pub fn generate_module(sets: &[ValueSet], device_model: &[DeviceModelRow]) -> String {
    let mut tokens = proc_macro2::TokenStream::new();

    // Each error type only exists if some set can actually return it.
    if sets.iter().any(|set| set.other_capacity.is_none()) {
        tokens.extend(generate_unknown_value());
    }

    if sets.iter().any(|set| set.other_capacity.is_some()) {
        tokens.extend(generate_value_too_long());
    }

    for set in sets {
        tokens.extend(generate_value_set(set));
    }

    if !device_model.is_empty() {
        tokens.extend(generate_device_model(device_model));
    }

    let module_doc = quote! {
        #![doc = " Value sets the specification defines for fields its JSON schemas"]
        #![doc = " type as bare strings."]
        #![doc = ""]
        #![doc = " A field like `Variable.name` or `SecurityEventNotification.type` is"]
        #![doc = " just `{\"type\": \"string\", \"maxLength\": N}` in the schema, with the"]
        #![doc = " permitted values listed in a specification appendix instead. Those"]
        #![doc = " appendix tables are vendored under `csv/` and generated into this"]
        #![doc = " module."]
        #![doc = ""]
        #![doc = " Nothing here changes a wire type. OCPP permits vendor-specific"]
        #![doc = " values for these fields, so the message structs keep their"]
        #![doc = " `heapless::String` fields and these enums sit alongside:"]
        #![doc = ""]
        #![doc = " # Open and closed value sets"]
        #![doc = ""]
        #![doc = " The sets a deployment realistically extends -- components,"]
        #![doc = " variables, configuration keys -- are *open*: they carry an"]
        #![doc = " `Other` variant, so a third-party value survives a"]
        #![doc = " deserialize/serialize round trip. Use `from_wire_or_other` to"]
        #![doc = " accept any value, `from_wire` to ask whether a value is one the"]
        #![doc = " specification defines, and `is_standardized` to tell them apart"]
        #![doc = " afterwards. Their equality and hashing compare the wire string,"]
        #![doc = " so `Other(\"X\")` and the `X` variant are one value."]
        #![doc = ""]
        #![doc = " The rest are *closed*: one byte, `Copy`, and `from_wire`"]
        #![doc = " returning `None` is the only outcome for an unrecognized value."]
        #![doc = " An open set is neither `Copy` nor one byte -- it is as wide as"]
        #![doc = " the field's `maxLength` -- which is why it is opt-in per set."]
        #![doc = ""]
        #![doc = " ```ignore"]
        #![doc = " use ocpp_types::v21::common::Variable;"]
        #![doc = " use ocpp_types::v21::standard::VariableName;"]
        #![doc = ""]
        #![doc = " let variable = Variable {"]
        #![doc = "     name: heapless::String::try_from(VariableName::HeartbeatInterval.as_str()).unwrap(),"]
        #![doc = "     custom_data: None,"]
        #![doc = "     instance: None,"]
        #![doc = " };"]
        #![doc = " ```"]
    };

    let file: syn::File = syn::parse2(quote! { #module_doc #tokens })
        .expect("generated `standard` module should parse");

    format!(
        "{}{}",
        crate::rust::GENERATED_FILE_BANNER,
        prettyplease::unparse(&file)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(files: &'static [(&'static str, &'static str)]) -> impl FnMut(&str) -> Option<String> {
        move |name| {
            files
                .iter()
                .find(|(file, _)| *file == name)
                .map(|(_, content)| content.to_string())
        }
    }

    const SIMPLE: ValueSetSpec = ValueSetSpec {
        name: "SecurityEvent",
        files: &["security_events.csv"],
        value_column: "Security Event",
        doc_column: Some("Description"),
        layout: RowLayout::Flat,
        metadata: &[MetadataColumnSpec {
            column: "Critical",
            accessor: "is_critical",
            kind: MetadataKind::YesNo,
            doc: "Critical.",
        }],
        doc: "Security events.",
    other_capacity: None,
    };

    #[test]
    fn resolves_values_with_docs_and_yes_no_metadata() {
        let set = resolve(
            &SIMPLE,
            reader(&[(
                "security_events.csv",
                "Security Event;Description;Critical\n\
                 FirmwareUpdated;The firmware is updated;Yes\n\
                 SettingSystemTime;Time changed;No\n",
            )]),
        )
        .expect("value set");

        assert_eq!(set.values.len(), 2);
        assert_eq!(set.values[0].ident, "FirmwareUpdated");
        assert_eq!(set.values[0].raw, "FirmwareUpdated");
        assert_eq!(set.values[0].doc.as_deref(), Some("The firmware is updated"));
        assert_eq!(set.values[0].metadata[0].as_deref(), Some("Yes"));
        assert_eq!(set.values[1].metadata[0].as_deref(), Some("No"));
    }

    /// The 2.x `Model` variable's description carries a run of four literal
    /// tabs from the spreadsheet it was exported from. Left alone they land
    /// in a `///` comment in the published crate and trip
    /// `clippy::tabs_in_doc_comments`.
    #[test]
    fn collapses_spreadsheet_whitespace_in_a_description() {
        let set = resolve(
            &SIMPLE,
            reader(&[(
                "security_events.csv",
                "Security Event;Description;Critical\n\
                 FirmwareUpdated;level internal \t\t\t\tvariation not affecting  behaviour;Yes\n",
            )]),
        )
        .unwrap();

        assert_eq!(
            set.values[0].doc.as_deref(),
            Some("level internal variation not affecting behaviour")
        );
    }

    /// An open set trades `Copy` and derived serde for the ability to carry a
    /// vendor value. The generated shape has to differ accordingly, or it
    /// won't compile: a `serde(rename)` with no derive to consume it is an
    /// unknown attribute, and `const fn` can't bind `Other`'s string.
    #[test]
    fn an_open_value_set_emits_other_and_hand_rolled_serde() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "VariableName",
            files: &["variables.csv"],
            value_column: "Name",
            doc_column: Some("Description"),
            layout: RowLayout::Flat,
            metadata: &[MetadataColumnSpec {
                column: "Unit",
                accessor: "unit",
                kind: MetadataKind::Text,
                doc: "Unit.",
            }],
            doc: "Variables.",
            other_capacity: Some(50),
        };

        let set = resolve(
            &SPEC,
            reader(&[(
                "variables.csv",
                "Name;DataType;Unit;Description\nkWhOddity;decimal;kWh;Needs sanitizing\n",
            )]),
        )
        .unwrap();

        let code = generate_module(&[set], &[]);

        assert!(code.contains("Other(heapless::String<50usize>)"), "{code}");
        assert!(code.contains("pub fn from_wire_or_other"), "{code}");
        assert!(code.contains("pub fn is_standardized"), "{code}");
        assert!(code.contains("impl serde::Serialize for VariableName"), "{code}");
        assert!(code.contains("impl<'de> serde::Deserialize<'de> for VariableName"), "{code}");
        // Equality must go through the wire string, not the variant.
        assert!(code.contains("impl PartialEq for VariableName"), "{code}");
        // No derived serde, and therefore no `serde(rename)` to be consumed.
        assert!(!code.contains("derive(serde::Serialize, serde::Deserialize)"), "{code}");
        assert!(!code.contains(r#"serde(rename = "kWhOddity")"#), "{code}");
        // `Other` owns a string, so the enum cannot be `Copy`. Checked on the
        // enum's own derive rather than the whole module, since the error
        // types are `Copy` regardless.
        assert!(
            code.contains("#[derive(Debug, Clone)]\npub enum VariableName"),
            "{code}"
        );
    }

    /// A closed set must keep the cheaper shape -- one byte, `Copy`, derived
    /// serde -- so opening one set doesn't silently cost the others.
    #[test]
    fn a_closed_value_set_keeps_copy_and_derived_serde() {
        let set = resolve(
            &SIMPLE,
            reader(&[(
                "security_events.csv",
                "Security Event;Description;Critical\nFirmwareUpdated;Updated;Yes\n",
            )]),
        )
        .unwrap();

        let code = generate_module(&[set], &[]);

        assert!(
            code.contains(
                "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\n\
                 #[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]\n\
                 pub enum SecurityEvent"
            ),
            "{code}"
        );
        // Matched against emitted *code*, not the module doc -- that prose
        // explains the open/closed split and so names both shapes.
        assert!(!code.contains("Other(heapless::String"), "{code}");
        assert!(!code.contains("pub fn from_wire_or_other"), "{code}");
        assert!(!code.contains("pub struct ValueTooLong"), "{code}");
        assert!(code.contains("pub const fn as_str(&self) -> &'static str"), "{code}");
    }

    #[test]
    fn returns_none_when_the_version_has_no_such_table() {
        assert!(resolve(&SIMPLE, reader(&[])).is_none());
    }

    /// `connectorenumtype.csv` and `signingmethod.csv` carry values like
    /// `s309-1P-16A` and `ECDSA-secp256k1-SHA256`, which need sanitizing
    /// into identifiers while keeping the wire value for serde.
    #[test]
    fn sanitizes_a_value_that_is_not_a_valid_identifier() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "ConnectorType",
            files: &["connectorenumtype.csv"],
            value_column: "Value",
            doc_column: Some("Description"),
            layout: RowLayout::Flat,
            metadata: &[],
            doc: "Connectors.",
        other_capacity: None,
        };

        let set = resolve(
            &SPEC,
            reader(&[(
                "connectorenumtype.csv",
                "Value;Description\ns309-1P-16A;IEC 60309\ncCCS1-NACS;Combo\n",
            )]),
        )
        .expect("value set");

        assert_eq!(set.values[0].ident, "S3091P16A");
        assert_eq!(set.values[0].raw, "s309-1P-16A");
        assert_eq!(set.values[1].ident, "CCCS1NACS");
    }

    /// `reason_codes.csv` interleaves group headings (`Charging Profiles;;;`)
    /// with the codes belonging to them.
    #[test]
    fn grouped_layout_skips_headings_and_carries_the_group_onto_values() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "ReasonCode",
            files: &["reason_codes.csv"],
            value_column: "Reason code",
            doc_column: Some("Description"),
            layout: RowLayout::Grouped { group_column: "Group" },
            metadata: &[],
            doc: "Reason codes.",
        other_capacity: None,
        };

        let set = resolve(
            &SPEC,
            reader(&[(
                "reason_codes.csv",
                "Group;Reason code;Description;Typically used for\n\
                 Charging Profiles;;;\n\
                 ;DuplicateProfile;Already exists;SetChargingProfile\n\
                 System Errors;;;\n\
                 ;InternalError;Something broke;\n",
            )]),
        )
        .expect("value set");

        assert_eq!(set.values.len(), 2);
        assert_eq!(set.values[0].ident, "DuplicateProfile");
        assert_eq!(set.values[0].group.as_deref(), Some("Charging Profiles"));
        assert_eq!(set.values[1].group.as_deref(), Some("System Errors"));
    }

    /// 2.x `variables.csv` lists `Timeout` twice, as a generic `decimal`
    /// and as a `BatterySwapCtrlr` `integer`. One wire value is one
    /// variant, so the duplicate must fold in rather than produce a second
    /// variant (which wouldn't compile).
    #[test]
    fn a_repeated_wire_value_becomes_one_variant_keeping_both_descriptions() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "VariableName",
            files: &["variables.csv"],
            value_column: "Name",
            doc_column: Some("Description"),
            layout: RowLayout::Flat,
            metadata: &[MetadataColumnSpec {
                column: "DataType",
                accessor: "data_type",
                kind: MetadataKind::Text,
                doc: "Data type.",
            }],
            doc: "Variables.",
        other_capacity: None,
        };

        let set = resolve(
            &SPEC,
            reader(&[(
                "variables.csv",
                "Name;DataType;Unit;Description\n\
                 Timeout;decimal;s;Generic timeout value\n\
                 Timeout;integer;s;For BatterySwapCtrlr\n",
            )]),
        )
        .expect("value set");

        assert_eq!(set.values.len(), 1);
        assert_eq!(set.values[0].metadata[0].as_deref(), Some("decimal"));

        let doc = set.values[0].doc.as_deref().unwrap();
        assert!(doc.contains("Generic timeout value"), "{doc}");
        assert!(doc.contains("For BatterySwapCtrlr"), "{doc}");
    }

    #[test]
    fn merges_two_files_into_one_value_set() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "AdditionalInfoType",
            files: &["a.csv", "b.csv"],
            value_column: "additionalInfo.type",
            doc_column: Some("Description"),
            layout: RowLayout::Flat,
            metadata: &[],
            doc: "Additional info.",
        other_capacity: None,
        };

        let set = resolve(
            &SPEC,
            reader(&[
                ("a.csv", "additionalInfo.type;Description\nEVCCID;The EVCCID\n"),
                ("b.csv", "additionalInfo.type;Description\nPspRef;Payment ref\n"),
            ]),
        )
        .expect("value set");

        assert_eq!(
            set.values.iter().map(|v| v.raw.as_str()).collect::<Vec<_>>(),
            ["EVCCID", "PspRef"]
        );
    }

    /// The device-model table writes `ACCurrent(Min/MaxSet)` to mean "the
    /// `ACCurrent` variable, its `MinSet`/`MaxSet` attributes". Taking that
    /// literally invents a variable name no station sends.
    #[test]
    fn splits_attribute_notation_off_a_variable_name() {
        assert_eq!(
            split_attribute_notation("ACCurrent(Min/MaxSet)"),
            ("ACCurrent", Some("Min/MaxSet"))
        );
        assert_eq!(
            split_attribute_notation("DCVoltage(Target)"),
            ("DCVoltage", Some("Target"))
        );
        assert_eq!(split_attribute_notation("ACCurrent"), ("ACCurrent", None));
    }

    #[test]
    fn device_model_rows_record_the_attribute_notation_separately_from_the_name() {
        let rows = resolve_device_model(
            "Specific Component;Variable;Instance;Required?;DataType;Unit;Description\n\
             ConnectedEV;ACCurrent(Min/MaxSet);;no;decimal;A;EV min/max AC current\n",
        );

        assert_eq!(rows[0].variable, "ACCurrent");
        assert_eq!(rows[0].attributes.as_deref(), Some("Min/MaxSet"));
    }

    /// `variables.csv` is not a superset of the device-model table: the
    /// latter lists ~100 real variables the former omits. Generating from
    /// the dedicated table alone silently drops them.
    #[test]
    fn merges_device_model_names_missing_from_the_dedicated_table() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "VariableName",
            files: &["variables.csv"],
            value_column: "Name",
            doc_column: Some("Description"),
            layout: RowLayout::Flat,
            metadata: &[],
            doc: "Variables.",
        other_capacity: None,
        };

        let mut set = resolve(
            &SPEC,
            reader(&[("variables.csv", "Name;DataType;Unit;Description\nACCurrent;decimal;A;Current\n")]),
        )
        .unwrap();

        let rows = resolve_device_model(
            "Specific Component;Variable;Instance;Required?;DataType;Unit;Description\n\
             <generic>;ACCurrent;;no;decimal;A;Current\n\
             SecurityCtrlr;OcppCsmsUrl;;yes;string;;CSMS URL\n\
             ConnectedEV;ACCurrent(Min/MaxSet);;no;decimal;A;Min/max\n",
        );

        let added = merge_device_model_names(&mut set, &rows, DeviceModelNames::Variable);

        // `ACCurrent` was already present, and the parenthesized row folds
        // onto the same name rather than adding a second variant.
        assert_eq!(added, ["OcppCsmsUrl"]);
        assert_eq!(
            set.values.iter().map(|v| v.raw.as_str()).collect::<Vec<_>>(),
            ["ACCurrent", "OcppCsmsUrl"]
        );
    }

    #[test]
    fn device_model_maps_the_generic_marker_to_no_component() {
        let rows = resolve_device_model(
            "Specific Component;Variable;Instance;Required?;DataType;Unit;Description\n\
             <generic>;ACCurrent;;no;decimal;A;RMS AC current\n\
             AuthCtrlr;Enabled;;yes;boolean;;Whether auth is enabled\n",
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].component, None);
        assert_eq!(rows[0].variable, "ACCurrent");
        assert_eq!(rows[0].unit.as_deref(), Some("A"));
        assert_eq!(rows[1].component.as_deref(), Some("AuthCtrlr"));
        assert_eq!(rows[1].instance, None);
    }

    #[test]
    fn generated_module_is_valid_rust_and_exposes_the_expected_api() {
        let set = resolve(
            &SIMPLE,
            reader(&[(
                "security_events.csv",
                "Security Event;Description;Critical\nFirmwareUpdated;Updated;Yes\n",
            )]),
        )
        .unwrap();

        let code = generate_module(&[set], &[]);

        assert!(code.contains("pub enum SecurityEvent"));
        assert!(code.contains("pub const ALL: &'static [Self]"));
        assert!(code.contains("pub const fn as_str"));
        assert!(code.contains("pub fn from_wire"));
        assert!(code.contains("pub const fn is_critical"));
        assert!(code.contains("impl core::str::FromStr for SecurityEvent"));
        assert!(code.contains("pub struct UnknownValue"));
    }

    /// Every value whose identifier differs from the wire string needs a
    /// serde rename, or serialization silently emits the Rust spelling.
    /// `kWh` is the subtle case: it *is* a valid identifier, but
    /// `rust_variant_ident` upper-cases the leading letter for
    /// `non_camel_case_types`, so `KWh` must still rename back to `kWh`.
    #[test]
    fn every_value_whose_ident_differs_from_the_wire_string_gets_a_serde_rename() {
        const SPEC: ValueSetSpec = ValueSetSpec {
            name: "Unit",
            files: &["units_of_measure.csv"],
            value_column: "Value",
            doc_column: None,
            layout: RowLayout::Flat,
            metadata: &[],
            doc: "Units.",
        other_capacity: None,
        };

        let set = resolve(
            &SPEC,
            reader(&[(
                "units_of_measure.csv",
                "Value;Description\nPercent;Ratio\nkWh;Energy\ns309-1P-16A;Odd\n",
            )]),
        )
        .unwrap();

        let code = generate_module(&[set], &[]);

        // Clean PascalCase value: identifier already matches the wire.
        assert!(!code.contains(r#"serde(rename = "Percent")"#), "{code}");
        // Leading-lowercase and punctuated values both need renames.
        assert!(code.contains(r#"serde(rename = "kWh")"#), "{code}");
        assert!(code.contains(r#"serde(rename = "s309-1P-16A")"#), "{code}");
    }

    /// A missing rename is a silent wire bug, so assert it structurally
    /// across every value rather than spot-checking: any value whose
    /// identifier was sanitized must carry a rename back to the raw string.
    #[test]
    fn no_sanitized_value_is_left_without_a_rename() {
        let set = resolve(
            &SIMPLE,
            reader(&[(
                "security_events.csv",
                "Security Event;Description;Critical\n\
                 FirmwareUpdated;Updated;Yes\n\
                 kWhOddity;Lowercase lead;No\n\
                 Some-Punctuated.Event;Punctuated;No\n",
            )]),
        )
        .unwrap();

        let code = generate_module(core::slice::from_ref(&set), &[]);

        for value in &set.values {
            if value.ident == value.raw {
                continue;
            }

            assert!(
                code.contains(&format!(r#"serde(rename = "{}")"#, value.raw)),
                "`{}` was sanitized to `{}` but has no serde rename",
                value.raw,
                value.ident
            );
        }
    }
}
