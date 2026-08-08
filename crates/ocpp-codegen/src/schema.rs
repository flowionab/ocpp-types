use crate::model::ConstParam;
use crate::model::Constraints;
use crate::model::GeneratedType;
use crate::model::OcppVersion;
use crate::model::ParsedSchema;
use crate::model::RustEnum;
use crate::model::RustField;
use crate::model::RustStruct;
use crate::model::RustType;
use crate::model::RustVariant;
use crate::model::TypeParam;
use crate::naming::CUSTOM_DATA_PARAM;
use crate::pool::TypePool;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;

/// Scans every schema in a batch for inline (`$ref`-less) string enums --
/// 1.6j's `status`, `type`, `unit`, etc. -- and returns the set of
/// pascal-cased field names that mean *different* things (different value
/// sets) on different messages, e.g. `status` on `SendLocalListResponse`
/// (`Accepted`/`Failed`/`NotSupported`/`VersionMismatch`) vs. on
/// `ReserveNowResponse` (`Accepted`/`Faulted`/`Occupied`/`Rejected`/
/// `Unavailable`).
///
/// Naming a generated type purely from the field name (see
/// [`crate::naming::pascal_case`]) would otherwise collapse every one of
/// these into a single shared type, silently keeping whichever message's
/// variants happened to be generated first and leaving the rest with the
/// wrong shape. Names in the returned set must be qualified with their
/// owning message/struct name instead; names *not* in the set are safe to
/// share under their bare field name, since every message that uses them
/// means the same thing (e.g. `idTagInfo`).
pub fn find_ambiguous_inline_enum_names(schemas: &[Value]) -> HashSet<String> {
    let mut signatures: HashMap<String, HashSet<Vec<String>>> = HashMap::new();

    for schema in schemas {
        let definitions = schema["definitions"].as_object().cloned().unwrap_or_default();
        let mut visited = HashSet::new();
        collect_inline_enum_signatures(schema, &definitions, &mut visited, &mut signatures);
    }

    signatures
        .into_iter()
        .filter_map(|(name, sigs)| (sigs.len() > 1).then_some(name))
        .collect()
}

fn collect_inline_enum_signatures(
    object_schema: &Value,
    definitions: &Map<String, Value>,
    visited: &mut HashSet<String>,
    signatures: &mut HashMap<String, HashSet<Vec<String>>>,
) {
    let Some(properties) = object_schema["properties"].as_object() else {
        return;
    };

    for (field_name, property) in properties {
        visit_property_for_signatures(field_name, property, definitions, visited, signatures);
    }
}

fn visit_property_for_signatures(
    field_name: &str,
    property: &Value,
    definitions: &Map<String, Value>,
    visited: &mut HashSet<String>,
    signatures: &mut HashMap<String, HashSet<Vec<String>>>,
) {
    if let Some(reference) = property["$ref"].as_str() {
        if let Some(def_name) = reference.strip_prefix("#/definitions/") {
            if visited.insert(def_name.to_string()) {
                if let Some(def_schema) = definitions.get(def_name) {
                    collect_inline_enum_signatures(def_schema, definitions, visited, signatures);
                }
            }
        }
        return;
    }

    match property["type"].as_str() {
        Some("string") if property["enum"].is_array() => {
            let mut variants: Vec<String> = property["enum"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            variants.sort();

            signatures
                .entry(crate::naming::pascal_case(field_name))
                .or_default()
                .insert(variants);
        }
        Some("object") if property["properties"].is_object() => {
            collect_inline_enum_signatures(property, definitions, visited, signatures);
        }
        Some("array") => {
            let item_field_name = format!("{field_name}Item");
            visit_property_for_signatures(&item_field_name, &property["items"], definitions, visited, signatures);
        }
        _ => {}
    }
}

/// Default capacity for a `heapless::String<N>` backing a field with no
/// spec-given `maxLength`, when the caller doesn't override the const
/// generic parameter.
const DEFAULT_STRING_CAPACITY: usize = 1024;

/// Default capacity for a `heapless::Vec<T, N>` when the caller doesn't
/// override the const generic parameter -- both for arrays with no
/// spec-given `maxItems` and for those whose `maxItems` exceeds
/// [`MAX_INLINE_VEC_CAPACITY`].
///
/// Much smaller than the string default, because OCPP's telemetry messages
/// are arrays *of* arrays: a `MeterValuesRequest` holds meter values, each
/// holding sampled values, so this number is squared before it reaches the
/// message. At 16 that was 256 sampled values reserved inline (267 KB for
/// 1.6J's `MeterValuesRequest`); at 8 it is 64.
///
/// Not lowered further, though 4 would be another 4x: five measurands on one
/// meter value -- energy, power, current, voltage, state of charge -- is an
/// ordinary configuration, and a capacity of 4 would reject it. 8 leaves
/// headroom over what deployments actually send. Callers who know their own
/// bound can still name it.
const DEFAULT_VEC_CAPACITY: usize = 8;

/// A spec-bounded array larger than this becomes a caller-chosen const
/// generic rather than being inlined at its `maxItems`.
///
/// Inlining the spec's ceiling is what makes the charging-profile family
/// unusable without `alloc`: `chargingSchedulePeriod` declares
/// `maxItems: 1024`, and 2.1's `ChargingSchedulePeriod` is itself 12 KB
/// (two spec-bounded 20-element V2X curves), so one schedule reserves
/// ~12 MB by value before any nesting. Seventeen 2.1 arrays declare 1024.
///
/// Reserving the protocol ceiling is also not what OCPP asks of a station:
/// `SmartChargingCtrlr.PeriodsPerSchedule` is a *required* 2.x variable and
/// 1.6 has `ChargingScheduleMaxPeriods`, i.e. every station must declare its
/// own, lower, limit. A station that compiles in the capacity it advertises
/// is conformant; one that reserves 1024 periods it will never accept is
/// merely large.
///
/// Set below the smallest array worth parameterizing (2.1's 20-element V2X
/// curves), so arrays of 10 or fewer -- which cost little inline -- keep
/// their fixed capacity and don't add a const parameter to every ancestor.
const MAX_INLINE_VEC_CAPACITY: usize = 16;

/// A spec-bounded string longer than this becomes a caller-chosen const
/// generic rather than being inlined at its `maxLength`, defaulting to
/// [`DEFAULT_STRING_CAPACITY`] (clamped to the spec's own ceiling).
///
/// The same asymmetry the array rule fixes, for strings: the specification
/// states generous ceilings for fields that are usually short or absent, and
/// inlining them costs that ceiling on every value. 2.1 alone has
/// `signedMeterData` at 32,768, `ocspResult` at 18,000, `exiResponse` at
/// 17,000, five `certificate`/`certificateChain` fields at 10,000, and
/// sixteen at 2,000. `signedMeterData` sits inside `SampledValue`, so it is
/// multiplied by both the sampled-value and meter-value arrays -- which is
/// most of what made `TransactionEventRequest` megabytes.
///
/// Set at 512 so identifiers, hashes and short free text keep their exact
/// spec bound and add no parameter; only the genuinely large fields become
/// tunable.
///
/// The default is deliberately the same 1024 as an unbounded string rather
/// than something smaller: one number to reason about, and safe for the URLs,
/// meter readings and identifiers that make up most of these fields. It is
/// *not* enough for a PEM certificate chain -- a deployment doing Plug and
/// Charge must raise `certificate`, `certificateChain`, `csr`,
/// `signingCertificate`, `exiRequest`/`exiResponse` and `signedMeterData`
/// explicitly. Erring low here is deliberate: the cost of the small default
/// is a deserialize failure a Plug and Charge deployment hits immediately in
/// testing, while erring high costs every deployment that never sees a
/// certificate.
const MAX_INLINE_STRING_CAPACITY: usize = 512;

/// Schema properties whose *description* states an exact civil-time format
/// the schema itself never bounds, mapped to the crate type that models it.
///
/// Unlike `date-time`, these declare no `format`, so they can only be matched
/// by name -- which is more fragile than the `format` rule, and why
/// `schema::tests::the_civil_format_fields_still_exist_in_the_vendored_schemas`
/// fails loudly if a schema revision renames one. Without that guard a rename
/// would silently revert the field to a 1024-byte string.
///
/// 2.1's tariff conditions are the whole set: `startTimeOfDay`/`endTimeOfDay`
/// are `HH:MM` and `validFromDate`/`validToDate` are `YYYY-MM-DD`, with the
/// regex given in each property's own description. Four of them sit in
/// `TariffConditions`, multiplied through three tariff kinds and their price
/// arrays.
const CIVIL_FORMAT_FIELDS: &[(&str, &str)] = &[
    ("startTimeOfDay", "crate::OcppTimeOfDay"),
    ("endTimeOfDay", "crate::OcppTimeOfDay"),
    ("validFromDate", "crate::OcppDate"),
    ("validToDate", "crate::OcppDate"),
];

/// The 2.x vendor-extension property, whose type is the caller's choice
/// rather than the spec's shape. See [`SchemaParser::resolve_type`].
const CUSTOM_DATA_FIELD: &str = "customData";


pub struct SchemaParser<'a> {
    definitions: Map<String, Value>,
    pool: &'a mut TypePool,
    version: OcppVersion,
    /// Names currently being resolved within *this* schema file, so a
    /// self-referential definition doesn't recurse forever. Distinct from
    /// `pool`, which tracks types that are already fully generated
    /// (possibly by an earlier schema file).
    in_progress: HashSet<String>,
    /// Pascal-cased inline-enum field names that must be qualified with
    /// their owning struct name rather than shared bare, per
    /// [`find_ambiguous_inline_enum_names`]. Empty unless the caller
    /// pre-scanned a whole batch for cross-file collisions.
    ambiguous_inline_enum_names: &'a HashSet<String>,
}

impl<'a> SchemaParser<'a> {
    /// Parses a single schema file in isolation: nested types it pulls in
    /// via `$ref` are returned alongside it rather than shared with
    /// anything else. Use [`Self::parse_with_pool`] when generating many
    /// schema files together so shared definitions are deduplicated across
    /// them.
    pub fn parse(value: &Value, version: OcppVersion) -> anyhow::Result<ParsedSchema> {
        let mut pool = TypePool::new();
        let message = Self::parse_with_pool(value, &mut pool, version)?;

        Ok(ParsedSchema {
            message,
            types: pool.types().to_vec(),
        })
    }

    /// Parses a single schema file, registering any `$ref`-resolved types
    /// into `pool`. Calling this repeatedly with the same `pool` across
    /// multiple schema files is what gives cross-file deduplication: a
    /// definition already registered by an earlier file is reused instead
    /// of being generated again.
    ///
    /// Inline enum collisions across files (see
    /// [`find_ambiguous_inline_enum_names`]) aren't caught by this method on
    /// its own, since it only sees one schema at a time -- use
    /// [`Self::parse_with_pool_ambiguous`] with a batch-wide pre-scan when
    /// that matters (as [`crate::batch::generate_batch`] does).
    pub fn parse_with_pool(
        value: &Value,
        pool: &mut TypePool,
        version: OcppVersion,
    ) -> anyhow::Result<RustStruct> {
        Self::parse_with_pool_ambiguous(value, pool, version, &HashSet::new())
    }

    /// Same as [`Self::parse_with_pool`], but qualifies any inline enum
    /// whose field name is in `ambiguous_inline_enum_names` with its owning
    /// struct name instead of sharing it bare, so cross-file collisions
    /// (like 1.6j's `status` meaning something different on every response)
    /// don't collapse into one wrong shared type.
    pub fn parse_with_pool_ambiguous(
        value: &Value,
        pool: &mut TypePool,
        version: OcppVersion,
        ambiguous_inline_enum_names: &HashSet<String>,
    ) -> anyhow::Result<RustStruct> {
        let definitions = value["definitions"]
            .as_object()
            .cloned()
            .unwrap_or_default();

        let mut parser = SchemaParser {
            definitions,
            pool,
            version,
            in_progress: HashSet::new(),
            ambiguous_inline_enum_names,
        };

        let name = Self::message_name(value)?;
        let action = Self::action_name(&name);
        let description = Self::description(value);
        let fields = parser.parse_fields(&name, value)?;

        Ok(RustStruct {
            name,
            action: Some(action),
            description,
            fields,
        })
    }

    /// A schema's `description`, sanitized for use as a doc comment:
    /// `\r\n` (common in these schemas) normalized to `\n`, leading/
    /// trailing whitespace trimmed, and `[`/`]` escaped since spec text
    /// like `[RFC5646]` (a citation) or `[0-9]` (a character class)
    /// otherwise reads as rustdoc intra-doc link syntax and produces
    /// broken-link warnings for links that were never meant to be links.
    /// `None` if absent or blank.
    fn description(schema: &Value) -> Option<String> {
        let raw = schema["description"].as_str()?;
        let normalized = raw.replace("\r\n", "\n");
        let trimmed = normalized.trim();

        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.replace('[', "\\[").replace(']', "\\]"))
        }
    }

    /// 1.6j schemas carry the message name in `title`; 2.0.1/2.1 schemas
    /// omit `title` and put it as the last `:`-separated segment of `$id`
    /// instead (e.g. `urn:OCPP:Cp:2:2020:3:BootNotificationRequest`).
    fn message_name(value: &Value) -> anyhow::Result<String> {
        if let Some(title) = value["title"].as_str() {
            return Ok(title.to_string());
        }

        if let Some(id) = value["$id"].as_str() {
            // `rsplit` always yields at least one element, even with no
            // `:` in `id`, so this is infallible.
            let name = id.rsplit(':').next().unwrap();
            return Ok(name.to_string());
        }

        anyhow::bail!("schema has neither a `title` nor a usable `$id`")
    }

    fn action_name(title: &str) -> String {
        title
            .strip_suffix("Request")
            .or_else(|| title.strip_suffix("Response"))
            .unwrap_or(title)
            .to_string()
    }

    /// Field names that carry OCPP protocol semantics get a dedicated
    /// newtype from that version's `primitives` module instead of a plain
    /// bounded string, so the type system distinguishes e.g. an `IdTag`
    /// from an arbitrary string. These are version-specific: `idTag` is a
    /// 1.6J concept with no equivalent shape in 2.0.1/2.1 (which use the
    /// structurally different `IdTokenType`).
    fn wrapper_type_for(version: OcppVersion, field_name: &str) -> Option<&'static str> {
        match (version, field_name) {
            (OcppVersion::V16, "idTag") => Some("IdTag"),
            _ => None,
        }
    }

    fn parse_fields(&mut self, owner: &str, schema: &Value) -> anyhow::Result<Vec<RustField>> {
        let required: HashSet<&str> = schema["required"]
            .as_array()
            .map(|values| values.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        let mut fields = Vec::new();

        if let Some(properties) = schema["properties"].as_object() {
            for (name, property) in properties {
                let ty = self.parse_field_type(owner, name, property)?;

                fields.push(RustField {
                    optional: !required.contains(name.as_str()),
                    name: name.to_string(),
                    description: Self::description(property),
                    constraints: Self::constraints(property),
                    ty,
                });
            }
        }

        Ok(fields)
    }

    /// The value constraints a property states, verbatim -- what a
    /// conformant value may be, independent of how the generated type
    /// happens to store it. Consumed by [`crate::validate`], which emits
    /// checks for the ones the type cannot enforce on its own.
    ///
    /// A `$ref` property carries none of its own: OCPP's definitions are
    /// objects and enums, whose constraints live on their properties and
    /// are picked up when *those* are parsed.
    fn constraints(property: &Value) -> Constraints {
        let number = |key: &str| property[key].as_f64();
        let count = |key: &str| property[key].as_u64().map(|value| value as usize);

        // Only arrays have items to constrain, and an array whose items
        // state nothing (the usual `$ref` case) keeps `None` rather than a
        // box full of `None`s.
        let item = property
            .get("items")
            .map(Self::constraints)
            .filter(|item: &Constraints| !item.is_empty())
            .map(Box::new);

        Constraints {
            max_length: count("maxLength"),
            min_items: count("minItems"),
            max_items: count("maxItems"),
            minimum: number("minimum"),
            maximum: number("maximum"),
            multiple_of: number("multipleOf"),
            item,
        }
    }

    fn parse_field_type(
        &mut self,
        owner: &str,
        field_name: &str,
        schema: &Value,
    ) -> anyhow::Result<RustType> {
        if let Some(wrapper) = Self::wrapper_type_for(self.version, field_name) {
            return Ok(RustType::Primitive(wrapper.to_string()));
        }

        self.resolve_type(owner, field_name, schema)
    }

    fn resolve_type(&mut self, owner: &str, field_name: &str, schema: &Value) -> anyhow::Result<RustType> {
        // 2.x hangs an optional `customData` on nearly every object -- 151
        // structs in 2.1 -- as a vendor extension point. Inlining the spec's
        // shape costs 264 bytes at every one of those nodes, and by-value
        // nesting multiplies that by the whole type graph rather than paying
        // it once, which is most of what makes `ChargingProfile` large.
        //
        // So the field's type is the caller's, defaulting to a zero-sized
        // stand-in. Deliberately ONE shared parameter name for every
        // `customData` in the version, not the usual owner-qualified one:
        // `collect_type_params` dedupes by name, so a struct containing ten
        // others that each carry `customData` still declares a single
        // parameter. Owner-qualified names would give it ten, and the count
        // would compound at every level.
        if let Some((_, path)) = CIVIL_FORMAT_FIELDS
            .iter()
            .find(|(name, _)| *name == field_name)
        {
            return Ok(RustType::CrateType((*path).to_string()));
        }

        if field_name == CUSTOM_DATA_FIELD {
            // Resolve the reference anyway, for its side effect: it registers
            // the specification's own `CustomData` shape in the pool so the
            // struct is still generated. Nothing references it now, but a
            // caller opting back in names it -- `ChargingProfile<CustomData>`
            // -- and it would otherwise vanish from the crate entirely.
            if let Some(reference) = schema["$ref"].as_str() {
                self.resolve_ref(reference)?;
            }

            return Ok(RustType::Any(TypeParam {
                name: CUSTOM_DATA_PARAM.to_string(),
                default: "crate::NoCustomData".to_string(),
            }));
        }

        if let Some(reference) = schema["$ref"].as_str() {
            return self.resolve_ref(reference);
        }

        Ok(match schema["type"].as_str() {
            // An inline (`$ref`-less) enum, e.g. 1.6j's
            // `chargingProfilePurpose`. 2.x always defines enums via `$ref`
            // instead, but 1.6j frequently inlines them, so without this
            // they'd otherwise be treated as plain (unbounded) strings.
            //
            // The bare field name is ambiguous when the same name means
            // different things on different messages (1.6j's `status`,
            // `type`, `unit`, ...) -- qualify those with the owning struct
            // name so each message gets its own correctly-shaped enum
            // instead of silently sharing (or overwriting) another
            // message's.
            // Every version's timestamps are `{"type": "string", "format":
            // "date-time"}` and none states a `maxLength`, so as strings
            // they each took the 1024-byte unbounded default -- 142 fields
            // across the three versions. `OcppTimestamp` is 16 bytes, needs
            // no const parameter, and is comparable, which a string is not.
            Some("string") if schema["format"].as_str() == Some("date-time") => {
                RustType::CrateType("crate::OcppTimestamp".to_string())
            }

            Some("string") if schema["enum"].is_array() => {
                let candidate = crate::naming::pascal_case(field_name);
                let rust_name = if self.ambiguous_inline_enum_names.contains(&candidate) {
                    format!("{owner}{candidate}")
                } else {
                    candidate
                };
                self.resolve_named_type(rust_name, schema)?
            }

            // A `heapless::String<N>` needs a capacity. When the spec
            // states one (`maxLength`), use it; otherwise expose a const
            // generic parameter (default `DEFAULT_STRING_CAPACITY`) so the
            // caller picks the bound instead of us guessing one -- or, with
            // the `alloc` feature, this becomes a plain growable `String`.
            Some("string") => match schema["maxLength"].as_u64() {
                // The string mirror of the array rule below: a large
                // spec-bounded string becomes a caller-chosen capacity rather
                // than being inlined at its ceiling.
                Some(max) if max as usize > MAX_INLINE_STRING_CAPACITY => {
                    RustType::UnboundedString(ConstParam {
                        name: crate::naming::const_param_name(owner, field_name),
                        default: DEFAULT_STRING_CAPACITY.min(max as usize),
                    })
                }
                Some(max) => RustType::BoundedString(max as usize),
                None => RustType::UnboundedString(ConstParam {
                    name: crate::naming::const_param_name(owner, field_name),
                    default: DEFAULT_STRING_CAPACITY,
                }),
            },

            Some("boolean") => RustType::Bool,

            Some("integer") => RustType::Integer,

            Some("number") => RustType::Number,

            // Same idea as the unbounded-string case above, but for
            // `heapless::Vec<T, N>` / `maxItems`. The item type is resolved
            // under a distinct field-name suffix so an (unlikely) unbounded
            // array of unbounded items gets two independent params instead
            // of accidentally colliding on one.
            Some("array") => {
                let item_field_name = format!("{field_name}Item");
                match schema["maxItems"].as_u64() {
                    // Large spec-bounded arrays are parameterized like
                    // unbounded ones (see `MAX_INLINE_VEC_CAPACITY`): the
                    // ceiling stays reachable, but the default is one a
                    // no-`alloc` target can actually hold.
                    Some(max) if max as usize > MAX_INLINE_VEC_CAPACITY => {
                        let item_ty = self.resolve_type(owner, &item_field_name, &schema["items"])?;
                        RustType::UnboundedVec(
                            Box::new(item_ty),
                            ConstParam {
                                name: crate::naming::const_param_name(owner, field_name),
                                default: DEFAULT_VEC_CAPACITY.min(max as usize),
                            },
                        )
                    }
                    Some(max) => {
                        let item_ty = self.resolve_type(owner, &item_field_name, &schema["items"])?;
                        RustType::Vec(Box::new(item_ty), max as usize)
                    }
                    None => {
                        let item_ty = self.resolve_type(owner, &item_field_name, &schema["items"])?;
                        RustType::UnboundedVec(
                            Box::new(item_ty),
                            ConstParam {
                                name: crate::naming::const_param_name(owner, field_name),
                                default: DEFAULT_VEC_CAPACITY,
                            },
                        )
                    }
                }
            }

            // An inline (`$ref`-less) nested object, e.g. 1.6j's
            // `idTagInfo`/`csChargingProfiles`. 2.x always defines these
            // via `$ref` instead, but 1.6j sometimes inlines them. Named by
            // field name alone (not owner-qualified), so the same field
            // name/shape appearing on multiple messages -- `idTagInfo` on
            // both `AuthorizeResponse` and `StartTransactionResponse` --
            // naturally shares one generated type instead of duplicating.
            Some("object") if schema["properties"].is_object() => {
                self.resolve_named_type(crate::naming::pascal_case(field_name), schema)?
            }

            // A property with no `type` and no `$ref` at all -- 2.0.1/2.1's
            // `DataTransfer.data`, which the spec leaves open to
            // implementation. There's no `no_std` type that models
            // arbitrary JSON, and mapping it to `()` would make the field
            // structurally unable to carry the payload it exists for, so
            // the caller picks the type (see `RustType::Any`).
            None => RustType::Any(TypeParam {
                name: crate::naming::type_param_name(owner, field_name),
                default: "()".to_string(),
            }),

            // A `type` the generator doesn't recognize. Nothing sensible to
            // generate, and (unlike the untyped case above) no reason to
            // think a caller-chosen payload is what the schema meant.
            _ => RustType::Unknown,
        })
    }

    /// Resolves a local `#/definitions/Foo` pointer, generating the
    /// referenced type (struct or enum) into the shared pool the first time
    /// it's seen -- by this schema file or an earlier one.
    fn resolve_ref(&mut self, reference: &str) -> anyhow::Result<RustType> {
        let Some(def_name) = reference.strip_prefix("#/definitions/") else {
            return Ok(RustType::Unknown);
        };

        let Some(def_schema) = self.definitions.get(def_name).cloned() else {
            return Ok(RustType::Unknown);
        };

        let rust_name = Self::type_name_for(def_name, &def_schema);

        self.resolve_named_type(rust_name, &def_schema)
    }

    /// Generates a struct or enum into the shared pool under `rust_name`
    /// the first time it's seen -- by an earlier `$ref`, an earlier inline
    /// schema with the same synthesized name, or an earlier schema file
    /// entirely -- returning a reference to it either way. Shared by
    /// `$ref` resolution ([`Self::resolve_ref`]) and inline (`$ref`-less)
    /// object/enum schemas (in [`Self::resolve_type`]).
    ///
    /// Callers are responsible for qualifying `rust_name` up front (see
    /// [`find_ambiguous_inline_enum_names`]) when the same candidate name
    /// could mean different things on different messages -- this method
    /// itself just registers-or-reuses whatever name it's given, and still
    /// errors loudly (via [`TypePool::register`]) if two truly distinct
    /// shapes ever do land on the same name.
    fn resolve_named_type(&mut self, rust_name: String, schema: &Value) -> anyhow::Result<RustType> {
        if self.pool.contains(&rust_name) || !self.in_progress.insert(rust_name.clone()) {
            return Ok(RustType::Local(rust_name));
        }

        let generated = if let Some(values) = schema["enum"].as_array() {
            let variants = values
                .iter()
                .filter_map(Value::as_str)
                .map(|raw| RustVariant {
                    ident: crate::naming::rust_variant_ident(raw),
                    raw: raw.to_string(),
                })
                .collect();

            GeneratedType::Enum(RustEnum {
                name: rust_name.clone(),
                description: Self::description(schema),
                variants,
            })
        } else {
            let fields = self.parse_fields(&rust_name, schema)?;

            GeneratedType::Struct(RustStruct {
                name: rust_name.clone(),
                action: None,
                description: Self::description(schema),
                fields,
            })
        };

        self.pool.register(rust_name.clone(), generated)?;

        Ok(RustType::Local(rust_name))
    }

    fn type_name_for(def_name: &str, def_schema: &Value) -> String {
        def_schema["javaType"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                def_name
                    .strip_suffix("Type")
                    .unwrap_or(def_name)
                    .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn property_with_no_declared_type_becomes_a_caller_chosen_type_param() {
        // 2.0.1/2.1's `DataTransfer.data`: "Data without specified length or
        // format ... open to implementation". Mapping this to `()` (as the
        // generator used to) makes the field structurally incapable of
        // carrying the payload it exists for.
        let schema = json!({
            "title": "DataTransferRequest",
            "type": "object",
            "properties": {
                "data": { "description": "Data without specified length or format." }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V201).unwrap();

        let data = parsed.message.fields.iter().find(|f| f.name == "data").unwrap();
        assert_eq!(
            data.ty,
            RustType::Any(TypeParam {
                name: "DataTransferRequestData".to_string(),
                default: "()".to_string(),
            })
        );
    }

    #[test]
    fn action_name_strips_request_suffix() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.action.as_deref(), Some("Authorize"));
    }

    #[test]
    fn action_name_strips_response_suffix() {
        let schema = json!({
            "title": "AuthorizeResponse",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.action.as_deref(), Some("Authorize"));
    }

    #[test]
    fn falls_back_to_id_when_title_is_absent() {
        let schema = json!({
            "$id": "urn:OCPP:Cp:2:2020:3:BootNotificationRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.name, "BootNotificationRequest");
        assert_eq!(parsed.message.action.as_deref(), Some("BootNotification"));
    }

    #[test]
    fn message_level_description_becomes_struct_doc() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "Sent by the Charge Point to the CSMS.",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Sent by the Charge Point to the CSMS.")
        );
    }

    #[test]
    fn missing_description_is_none() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.description, None);
    }

    #[test]
    fn field_level_description_becomes_field_doc() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "maxLength": 20,
                    "description": "The reason for sending this message."
                }
            },
            "required": ["reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.fields[0].description.as_deref(),
            Some("The reason for sending this message.")
        );
    }

    #[test]
    fn crlf_in_description_is_normalized_and_trimmed() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "  Line one.\r\nLine two.\r\n  ",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Line one.\nLine two.")
        );
    }

    #[test]
    fn square_brackets_in_description_are_escaped_for_rustdoc() {
        // Spec text like `[RFC5646]` (a citation) or `[0-9]` (a character
        // class) is otherwise misread as rustdoc intra-doc link syntax,
        // producing broken-link warnings for links that were never meant
        // to be links.
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "Language, see [RFC5646] and digits [0-9].",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Language, see \\[RFC5646\\] and digits \\[0-9\\].")
        );
    }

    #[test]
    fn nested_struct_and_enum_descriptions_are_captured() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "definitions": {
                "ChargingStationType": {
                    "javaType": "ChargingStation",
                    "description": "The physical charging station.",
                    "type": "object",
                    "properties": {},
                    "required": []
                },
                "BootReasonEnumType": {
                    "javaType": "BootReasonEnum",
                    "description": "Reason for sending this message.",
                    "type": "string",
                    "enum": ["PowerUp"]
                }
            },
            "properties": {
                "chargingStation": { "$ref": "#/definitions/ChargingStationType" },
                "reason": { "$ref": "#/definitions/BootReasonEnumType" }
            },
            "required": ["chargingStation", "reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        let charging_station = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "ChargingStation" => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            charging_station.description.as_deref(),
            Some("The physical charging station.")
        );

        let boot_reason = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "BootReasonEnum" => Some(e),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            boot_reason.description.as_deref(),
            Some("Reason for sending this message.")
        );
    }

    #[test]
    fn id_tag_field_maps_to_wrapper_type() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {
                "idTag": {
                    "type": "string",
                    "maxLength": 20
                }
            },
            "required": ["idTag"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(matches!(&parsed.message.fields[0].ty, RustType::Primitive(name) if name == "IdTag"));
    }

    #[test]
    fn id_tag_wrapper_is_specific_to_v16_and_does_not_apply_to_other_versions() {
        // 2.0.1/2.1 don't have a bare `idTag` string field with this
        // meaning (they use the structurally different `IdTokenType`), so
        // the V16-only wrapper mapping must not accidentally fire for them.
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {
                "idTag": {
                    "type": "string",
                    "maxLength": 20
                }
            },
            "required": ["idTag"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V201).unwrap();

        assert!(matches!(
            &parsed.message.fields[0].ty,
            RustType::BoundedString(20)
        ));
    }

    #[test]
    fn field_not_in_required_is_optional() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": { "type": "string" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(parsed.message.fields[0].optional);
    }

    #[test]
    fn field_in_required_is_not_optional() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": { "type": "string" }
            },
            "required": ["reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(!parsed.message.fields[0].optional);
    }

    #[test]
    fn ref_to_object_definition_generates_nested_struct() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "definitions": {
                "ChargingStationType": {
                    "javaType": "ChargingStation",
                    "type": "object",
                    "properties": {
                        "model": { "type": "string", "maxLength": 20 },
                        "modem": { "type": "string", "maxLength": 5 }
                    },
                    "required": ["model"]
                }
            },
            "properties": {
                "chargingStation": { "$ref": "#/definitions/ChargingStationType" }
            },
            "required": ["chargingStation"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "ChargingStation")
        );

        let nested = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "ChargingStation" => Some(s),
                _ => None,
            })
            .expect("nested struct should be generated");

        assert!(nested.action.is_none());
        assert_eq!(nested.fields.len(), 2);
        let model_field = nested.fields.iter().find(|f| f.name == "model").unwrap();
        assert!(!model_field.optional);
        let modem_field = nested.fields.iter().find(|f| f.name == "modem").unwrap();
        assert!(modem_field.optional);
    }

    #[test]
    fn ref_to_enum_definition_generates_enum_with_sanitized_variants() {
        let schema = json!({
            "title": "NotifyEVChargingNeedsRequest",
            "type": "object",
            "definitions": {
                "MeasurandEnumType": {
                    "javaType": "MeasurandEnum",
                    "type": "string",
                    "enum": ["Voltage.Maximum", "Power.Active.Import"]
                }
            },
            "properties": {
                "measurand": { "$ref": "#/definitions/MeasurandEnumType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "MeasurandEnum")
        );

        let generated_enum = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "MeasurandEnum" => Some(e),
                _ => None,
            })
            .expect("enum should be generated");

        let variant = generated_enum
            .variants
            .iter()
            .find(|v| v.raw == "Voltage.Maximum")
            .unwrap();

        assert_eq!(variant.ident, "VoltageMaximum");
    }

    #[test]
    fn repeated_ref_to_same_definition_is_only_generated_once() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "definitions": {
                "CustomDataType": {
                    "javaType": "CustomData",
                    "type": "object",
                    "properties": {
                        "vendorId": { "type": "string", "maxLength": 255 }
                    },
                    "required": ["vendorId"]
                }
            },
            "properties": {
                "someData": { "$ref": "#/definitions/CustomDataType" },
                "otherData": { "$ref": "#/definitions/CustomDataType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        let count = parsed
            .types
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "CustomData"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn definition_name_falls_back_to_stripped_type_suffix_without_java_type() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "definitions": {
                "ModemType": {
                    "type": "object",
                    "properties": {
                        "iccid": { "type": "string", "maxLength": 20 }
                    },
                    "required": []
                }
            },
            "properties": {
                "modem": { "$ref": "#/definitions/ModemType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "Modem"));
    }

    #[test]
    fn array_with_max_items_becomes_bounded_vec_of_primitives() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string", "maxLength": 10 },
                    "maxItems": 5
                }
            },
            "required": ["tags"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::Vec(inner, cap) => {
                assert_eq!(*cap, 5);
                assert!(matches!(**inner, RustType::BoundedString(10)));
            }
            other => panic!("expected Vec, got {other:?}"),
        }
    }

    #[test]
    fn array_without_max_items_becomes_an_unbounded_vec_with_a_named_param() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string", "maxLength": 10 }
                }
            },
            "required": ["tags"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(inner, param) => {
                assert_eq!(param.name, "SOME_REQUEST_TAGS_CAP");
                assert_eq!(param.default, 8);
                assert!(matches!(**inner, RustType::BoundedString(10)));
            }
            other => panic!("expected UnboundedVec, got {other:?}"),
        }
    }

    /// [`CIVIL_FORMAT_FIELDS`] matches on field *name*, because these
    /// properties declare no `format` for the generator to key off. That is
    /// fragile in one specific way: if a schema revision renames one, the
    /// mapping silently stops applying and the field reverts to a 1024-byte
    /// string with a const parameter -- a regression nothing else would
    /// notice. So assert the names still exist, and still look like the
    /// unbounded strings the mapping assumes.
    #[test]
    fn the_civil_format_fields_still_exist_in_the_vendored_schemas() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/ocpp2.1");
        let mut seen: HashSet<&str> = HashSet::new();

        fn visit(node: &Value, seen: &mut HashSet<&'static str>) {
            if let Some(properties) = node["properties"].as_object() {
                for (name, property) in properties {
                    if let Some((known, _)) = CIVIL_FORMAT_FIELDS
                        .iter()
                        .find(|(candidate, _)| candidate == name)
                    {
                        assert_eq!(
                            property["type"].as_str(),
                            Some("string"),
                            "`{name}` is no longer a string"
                        );
                        assert!(
                            property["maxLength"].is_null(),
                            "`{name}` now declares a maxLength; revisit the mapping"
                        );
                        seen.insert(known);
                    }
                }
            }

            match node {
                Value::Object(map) => map.values().for_each(|v| visit(v, seen)),
                Value::Array(items) => items.iter().for_each(|v| visit(v, seen)),
                _ => {}
            }
        }

        for entry in std::fs::read_dir(&root).expect("schemas/ocpp2.1 should exist") {
            let path = entry.unwrap().path();

            if path.extension().is_some_and(|ext| ext == "json") {
                let schema: Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                visit(&schema, &mut seen);
            }
        }

        for (name, _) in CIVIL_FORMAT_FIELDS {
            assert!(
                seen.contains(name),
                "`{name}` is mapped to a crate type but no longer appears in the 2.1 \
                 schemas -- the mapping is dead and the field it replaced is back to \
                 an unbounded string"
            );
        }
    }

    /// 2.1's `chargingSchedulePeriod` declares `maxItems: 1024`, and its
    /// element is 12 KB, so inlining the ceiling reserves ~12 MB per
    /// schedule. Above the threshold the capacity becomes the caller's, with
    /// the spec ceiling still reachable by naming it.
    #[test]
    fn a_large_spec_bounded_array_becomes_a_caller_chosen_capacity() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "periods": {
                    "type": "array",
                    "maxItems": 1024,
                    "items": { "type": "string", "maxLength": 10 }
                }
            },
            "required": ["periods"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(inner, param) => {
                assert_eq!(param.name, "SOME_REQUEST_PERIODS_CAP");
                assert_eq!(param.default, 8, "should default well below the spec ceiling");
                assert!(matches!(**inner, RustType::BoundedString(10)));
            }
            other => panic!("expected UnboundedVec for a 1024-item array, got {other:?}"),
        }
    }

    /// 2.1's `signedMeterData` states `maxLength: 32768` and sits inside
    /// `SampledValue`, so inlining the ceiling is multiplied by both the
    /// sampled-value and meter-value arrays above it.
    #[test]
    fn a_large_spec_bounded_string_becomes_a_caller_chosen_capacity() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "signedMeterData": { "type": "string", "maxLength": 32768 }
            },
            "required": ["signedMeterData"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedString(param) => {
                assert_eq!(param.name, "SOME_REQUEST_SIGNED_METER_DATA_CAP");
                assert_eq!(param.default, 1024, "should default well below the ceiling");
            }
            other => panic!("expected UnboundedString, got {other:?}"),
        }
    }

    /// An identifier or hash costs little inlined and keeps its exact spec
    /// bound, so no parameter appears where it wouldn't pay for itself.
    #[test]
    fn a_small_spec_bounded_string_keeps_its_exact_capacity() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "idToken": { "type": "string", "maxLength": 255 }
            },
            "required": ["idToken"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::BoundedString(255)),
            "got {:?}",
            parsed.message.fields[0].ty
        );
    }

    /// The default never exceeds the spec's own ceiling, so a caller relying
    /// on it can't build a value the field forbids.
    #[test]
    fn the_default_string_capacity_is_clamped_to_the_spec_ceiling() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "signature": { "type": "string", "maxLength": 800 }
            },
            "required": ["signature"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedString(param) => assert_eq!(param.default, 800),
            other => panic!("expected UnboundedString, got {other:?}"),
        }
    }

    /// A small array costs little inlined, and parameterizing it would add a
    /// const parameter to every ancestor struct for no benefit.
    #[test]
    fn a_small_spec_bounded_array_keeps_its_fixed_capacity() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "schedules": {
                    "type": "array",
                    "maxItems": 3,
                    "items": { "type": "string", "maxLength": 10 }
                }
            },
            "required": ["schedules"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Vec(_, 3)),
            "got {:?}",
            parsed.message.fields[0].ty
        );
    }

    /// The default never exceeds the spec's own ceiling. Uses a ceiling
    /// below `DEFAULT_VEC_CAPACITY` so the clamp is actually exercised
    /// rather than passing because the default happened to be smaller.
    #[test]
    fn the_default_capacity_is_clamped_to_the_spec_ceiling() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "curve": {
                    "type": "array",
                    "maxItems": 17,
                    "items": { "type": "number" }
                }
            },
            "required": ["curve"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(_, param) => assert_eq!(param.default, 8),
            other => panic!("expected UnboundedVec, got {other:?}"),
        }
    }

    #[test]
    fn string_without_max_length_becomes_an_unbounded_string_with_a_named_param() {
        // A `heapless::String<N>` needs a capacity, same as `heapless::Vec`;
        // without `maxLength` there's nothing spec-given to size it with,
        // so the caller picks it via a const generic (or gets a plain
        // `alloc::string::String` under the `alloc` feature) instead of us
        // guessing a bound.
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "timestamp": { "type": "string" }
            },
            "required": ["timestamp"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedString(param) => {
                assert_eq!(param.name, "SOME_REQUEST_TIMESTAMP_CAP");
                assert_eq!(param.default, 1024);
            }
            other => panic!("expected UnboundedString, got {other:?}"),
        }
    }

    #[test]
    fn unbounded_array_of_unbounded_strings_gets_two_distinct_params() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "notes": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["notes"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(inner, outer_param) => {
                assert_eq!(outer_param.name, "SOME_REQUEST_NOTES_CAP");
                match inner.as_ref() {
                    RustType::UnboundedString(inner_param) => {
                        assert_ne!(inner_param.name, outer_param.name);
                    }
                    other => panic!("expected UnboundedString, got {other:?}"),
                }
            }
            other => panic!("expected UnboundedVec, got {other:?}"),
        }
    }

    #[test]
    fn array_of_ref_items_generates_nested_type_and_local_vec() {
        let schema = json!({
            "title": "MeterValuesRequest",
            "type": "object",
            "definitions": {
                "MeterValueType": {
                    "javaType": "MeterValue",
                    "type": "object",
                    "properties": {
                        "timestamp": { "type": "string" }
                    },
                    "required": ["timestamp"]
                }
            },
            "properties": {
                "meterValue": {
                    "type": "array",
                    "items": { "$ref": "#/definitions/MeterValueType" },
                    "maxItems": 4
                }
            },
            "required": ["meterValue"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::Vec(inner, cap) => {
                assert_eq!(*cap, 4);
                assert!(matches!(&**inner, RustType::Local(name) if name == "MeterValue"));
            }
            other => panic!("expected Vec, got {other:?}"),
        }

        assert!(parsed
            .types
            .iter()
            .any(|t| matches!(t, GeneratedType::Struct(s) if s.name == "MeterValue")));
    }

    fn schema_with_shared_definition(title: &str) -> Value {
        // Deliberately not `customData`: that property is a type parameter
        // now, not a `$ref` to a pooled definition, so it would no longer
        // exercise sharing at all.
        json!({
            "title": title,
            "type": "object",
            "definitions": {
                "StatusInfoType": {
                    "javaType": "StatusInfo",
                    "type": "object",
                    "properties": {
                        "reasonCode": { "type": "string", "maxLength": 20 }
                    },
                    "required": ["reasonCode"]
                }
            },
            "properties": {
                "statusInfo": { "$ref": "#/definitions/StatusInfoType" }
            },
            "required": []
        })
    }

    #[test]
    fn parse_with_pool_shares_a_definition_seen_across_multiple_schema_files() {
        let mut pool = crate::pool::TypePool::new();

        let first = SchemaParser::parse_with_pool(
            &schema_with_shared_definition("FirstRequest"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();
        let second = SchemaParser::parse_with_pool(
            &schema_with_shared_definition("SecondRequest"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();

        assert!(matches!(&first.fields[0].ty, RustType::Local(name) if name == "StatusInfo"));
        assert!(matches!(&second.fields[0].ty, RustType::Local(name) if name == "StatusInfo"));

        let count = pool
            .types()
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "StatusInfo"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn inline_object_without_ref_generates_a_named_local_struct() {
        // Real 1.6j shape: `idTagInfo` on AuthorizeResponse is a nested
        // object defined inline, not via `$ref`/`definitions` (which 2.x
        // always uses instead).
        let schema = json!({
            "title": "AuthorizeResponse",
            "type": "object",
            "properties": {
                "idTagInfo": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "maxLength": 20 }
                    },
                    "required": ["status"]
                }
            },
            "required": ["idTagInfo"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "IdTagInfo")
        );

        let nested = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "IdTagInfo" => Some(s),
                _ => None,
            })
            .expect("inline object should generate a struct");

        assert_eq!(nested.fields.len(), 1);
        assert_eq!(nested.fields[0].name, "status");
    }

    #[test]
    fn inline_object_with_the_same_field_name_and_shape_is_shared_across_messages() {
        fn schema_with_id_tag_info(title: &str) -> Value {
            json!({
                "title": title,
                "type": "object",
                "properties": {
                    "idTagInfo": {
                        "type": "object",
                        "properties": {
                            "status": { "type": "string", "maxLength": 20 }
                        },
                        "required": ["status"]
                    }
                },
                "required": ["idTagInfo"]
            })
        }

        let mut pool = TypePool::new();

        SchemaParser::parse_with_pool(
            &schema_with_id_tag_info("AuthorizeResponse"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();
        SchemaParser::parse_with_pool(
            &schema_with_id_tag_info("StartTransactionResponse"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();

        let count = pool
            .types()
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "IdTagInfo"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn inline_enum_with_same_field_name_but_different_values_is_qualified_per_message() {
        // Real 1.6j bug: `SendLocalListResponse.status` and
        // `ReserveNowResponse.status` are both inline enums, but with
        // completely different value sets. Naming the generated type purely
        // from the field name (`Status`) would collapse them into one type,
        // silently keeping whichever message's variants were seen first.
        let send_local_list = json!({
            "title": "SendLocalListResponse",
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["Accepted", "Failed", "NotSupported", "VersionMismatch"]
                }
            },
            "required": ["status"]
        });
        let reserve_now = json!({
            "title": "ReserveNowResponse",
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["Accepted", "Faulted", "Occupied", "Rejected", "Unavailable"]
                }
            },
            "required": ["status"]
        });

        let ambiguous = find_ambiguous_inline_enum_names(&[send_local_list.clone(), reserve_now.clone()]);
        let mut pool = TypePool::new();

        let first =
            SchemaParser::parse_with_pool_ambiguous(&send_local_list, &mut pool, OcppVersion::V16, &ambiguous)
                .unwrap();
        let second =
            SchemaParser::parse_with_pool_ambiguous(&reserve_now, &mut pool, OcppVersion::V16, &ambiguous).unwrap();

        assert!(matches!(
            &first.fields[0].ty,
            RustType::Local(name) if name == "SendLocalListResponseStatus"
        ));
        assert!(matches!(
            &second.fields[0].ty,
            RustType::Local(name) if name == "ReserveNowResponseStatus"
        ));

        let first_enum = pool
            .types()
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "SendLocalListResponseStatus" => Some(e),
                _ => None,
            })
            .expect("SendLocalListResponseStatus enum should be generated");
        assert_eq!(first_enum.variants.len(), 4);

        let second_enum = pool
            .types()
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "ReserveNowResponseStatus" => Some(e),
                _ => None,
            })
            .expect("ReserveNowResponseStatus enum should be generated");
        assert_eq!(second_enum.variants.len(), 5);
    }

    #[test]
    fn inline_enum_with_same_field_name_and_same_values_is_shared_across_messages() {
        fn schema_with_status(title: &str) -> Value {
            json!({
                "title": title,
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["Accepted", "Rejected"]
                    }
                },
                "required": ["status"]
            })
        }

        let foo = schema_with_status("FooResponse");
        let bar = schema_with_status("BarResponse");
        let ambiguous = find_ambiguous_inline_enum_names(&[foo.clone(), bar.clone()]);
        let mut pool = TypePool::new();

        SchemaParser::parse_with_pool_ambiguous(&foo, &mut pool, OcppVersion::V16, &ambiguous).unwrap();
        SchemaParser::parse_with_pool_ambiguous(&bar, &mut pool, OcppVersion::V16, &ambiguous).unwrap();

        let count = pool
            .types()
            .iter()
            .filter(|t| matches!(t, GeneratedType::Enum(e) if e.name == "Status"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn inline_enum_without_ref_generates_a_named_local_enum() {
        let schema = json!({
            "title": "SetChargingProfileRequest",
            "type": "object",
            "properties": {
                "chargingProfilePurpose": {
                    "type": "string",
                    "enum": ["ChargePointMaxProfile", "TxDefaultProfile", "TxProfile"]
                }
            },
            "required": ["chargingProfilePurpose"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "ChargingProfilePurpose")
        );

        let generated_enum = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "ChargingProfilePurpose" => Some(e),
                _ => None,
            })
            .expect("inline enum should generate an enum");

        assert_eq!(generated_enum.variants.len(), 3);
    }
}
