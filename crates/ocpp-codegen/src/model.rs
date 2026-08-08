/// Which OCPP protocol version a schema file belongs to. Wrapper primitives
/// (see [`RustType::Primitive`]) are version-specific -- `IdTag` is a 1.6J
/// concept with no equivalent shape in 2.0.1/2.1 -- so parsing needs to know
/// which version's registry to consult.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppVersion {
    V16,
    V201,
    V21,
}

#[derive(Debug)]
pub struct ParsedSchema {
    /// The top-level request/response type described by the schema file.
    pub message: RustStruct,
    /// Nested types pulled in transitively through `$ref`, deduplicated by
    /// name. Order is insertion order (first reference wins).
    pub types: Vec<GeneratedType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratedType {
    Struct(RustStruct),
    Enum(RustEnum),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RustStruct {
    pub name: String,
    /// `Some(action)` for the message type at the root of a schema file;
    /// `None` for nested types reached via `$ref`, which don't have an
    /// OCPP action of their own.
    pub action: Option<String>,
    /// The schema's `description`, if any, rendered as a doc comment.
    pub description: Option<String>,
    pub fields: Vec<RustField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RustField {
    pub name: String,
    pub ty: RustType,
    pub optional: bool,
    /// The property schema's `description`, if any, rendered as a doc
    /// comment.
    pub description: Option<String>,
    /// The property schema's value constraints, for the generated
    /// `Validate` impl (see [`crate::validate`]). Kept separate from
    /// [`RustType`] because the two answer different questions: the type is
    /// how the value is *stored* (and a large `maxLength` deliberately
    /// isn't stored at its ceiling), while this is what the specification
    /// says a conformant value may be.
    pub constraints: Constraints,
}

/// The value constraints a schema property states, whether or not the
/// generated type happens to enforce them.
///
/// Every field is `None` when the schema is silent, which is the common
/// case -- most properties state nothing beyond their type.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Constraints {
    /// `maxLength`, in characters.
    pub max_length: Option<usize>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub multiple_of: Option<f64>,
    /// Constraints on this array's *items*, when the property is an array.
    /// `None` for anything else, and for arrays whose items state nothing.
    pub item: Option<Box<Constraints>>,
}

impl Constraints {
    /// Whether the schema stated anything at all here (including about
    /// this array's items).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RustEnum {
    pub name: String,
    /// The schema's `description`, if any, rendered as a doc comment.
    pub description: Option<String>,
    pub variants: Vec<RustVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RustVariant {
    /// Sanitized to a valid Rust identifier (e.g. `Voltage.Maximum` -> `VoltageMaximum`).
    pub ident: String,
    /// The raw string as it appears in the schema's `enum` list / on the wire.
    pub raw: String,
}

/// A const generic parameter a struct exposes for a field with no
/// spec-given bound. Named from the (owning struct, field) pair that
/// introduced it, e.g. `ChargingScheduleChargingSchedulePeriodCap`, so the
/// name stays unique and self-documenting even after being threaded up
/// through several levels of nesting.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstParam {
    pub name: String,
    pub default: usize,
}

/// A generic *type* parameter a struct exposes for a schema property that
/// declares no type at all -- 2.0.1/2.1's `DataTransfer.data`, which the
/// spec deliberately leaves "open to implementation". There is no single
/// Rust type that models arbitrary JSON in a `no_std`, non-`alloc` crate,
/// so the payload type is the caller's to choose (defaulting to `()`, i.e.
/// "this deployment sends no data"). Named from the (owning struct, field)
/// pair, e.g. `DataTransferRequestData`, on the same rationale as
/// [`ConstParam`].
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    /// The default type, rendered verbatim into `<T = ...>`.
    ///
    /// `()` for a property the spec leaves untyped ("this deployment sends
    /// nothing here"). `crate::NoCustomData` for `customData`, which needs a
    /// default that still *deserializes* -- a peer may send an extension at
    /// any time, and `()` accepts only `null`.
    pub default: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RustType {
    Bool,
    Integer,
    Number,

    BoundedString(usize),

    /// A hand-written type in `ocpp-types` itself, named by absolute path
    /// (e.g. `crate::OcppTimestamp`), for schema strings that carry a fixed
    /// format the crate models properly.
    ///
    /// These are version-independent -- a `date-time` means the same thing in
    /// 1.6J and 2.1 -- which is why they are addressed by crate path rather
    /// than through the per-version `primitives` module. Each replaces a
    /// string that stated no `maxLength` and so took the 1024-byte unbounded
    /// default, removing the size and the const parameter together.
    CrateType(String),

    /// A hand-written type in the target OCPP version's `primitives`
    /// module (e.g. `v16::primitives::IdTag`), referenced as `super::Name`
    /// from generated files, which all live one level below their version
    /// module.
    Primitive(String),

    /// A type generated into the same output file (from a `$ref`),
    /// referenced by its bare name. May itself be generic (see
    /// `crate::generics`); if so, the owning struct must also expose those
    /// same const params and forward them here.
    Local(String),

    /// A bounded array from a schema `array` with `maxItems`, rendered as
    /// `heapless::Vec<T, N>`.
    Vec(Box<RustType>, usize),

    /// A string with no `maxLength`. Under the `alloc` feature this is
    /// `alloc::string::String`; otherwise `heapless::String<N>`, where `N`
    /// is this const param (owned by the struct this field lives on, or
    /// forwarded from further down if this came from a nested field).
    UnboundedString(ConstParam),

    /// An array with no `maxItems`. Same alloc/const-generic split as
    /// [`RustType::UnboundedString`].
    UnboundedVec(Box<RustType>, ConstParam),

    /// A property the schema gives no type for, rendered as the caller-
    /// chosen type parameter described by this [`TypeParam`] (owned by the
    /// struct this field lives on, or forwarded from further down).
    Any(TypeParam),

    /// A `$ref` that couldn't be resolved at all -- rendered as `()`, since
    /// there's nothing to generate. Distinct from [`RustType::Any`]: that's
    /// a property the schema deliberately leaves open, this is a schema the
    /// generator failed to make sense of.
    Unknown,
}
