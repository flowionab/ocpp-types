//! Checking a payload against the parts of the OCPP specification the
//! types themselves cannot carry.
//!
//! Most spec limits are in the type: a field the schema bounds at
//! `maxLength: 20` is a `heapless::String<20>`, so an over-long value is
//! rejected at construction and never reaches the wire. Two categories
//! escape that:
//!
//! * **Bounds too large to inline.** A string bounded above 512 characters,
//!   or an array above 16 elements, would make every message that contains
//!   it enormous by value, so it is not stored at its spec ceiling. Under
//!   the `alloc` feature it becomes a plain `alloc::string::String` /
//!   `alloc::vec::Vec`, which has no bound at all; without `alloc` it is a
//!   `heapless` collection at a *caller-chosen* capacity, which may sit
//!   either side of the spec's. Either way the specification's own limit is
//!   no longer enforced by the type. `AuthorizeRequest::certificate` is one:
//!   `maxLength: 5500` in the schema, a growable `String` in the `alloc`
//!   build.
//! * **Constraints no collection expresses.** `minItems` (usually `1`, i.e.
//!   "must not be empty"), `minimum`/`maximum` on numbers, and 1.6J's
//!   `multipleOf` on charging limits. A `heapless::Vec<T, N>` can say "at
//!   most N"; it cannot say "at least one".
//!
//! [`Validate`] covers exactly those. It is implemented for every generated
//! message and nested type, recurses through nested structs and arrays, and
//! reports the first violation with the JSON path that reaches it:
//!
//! ```
//! # #[cfg(feature = "alloc")] {
//! use ocpp_types::v201::AuthorizeRequest;
//! use ocpp_types::v201::common::{IdToken, IdTokenEnum};
//! use ocpp_types::validate::{Validate, ValidationErrorKind};
//!
//! let request: AuthorizeRequest = AuthorizeRequest {
//!     certificate: Some("-".repeat(6000)),
//!     custom_data: None,
//!     id_token: IdToken {
//!         additional_info: None,
//!         custom_data: None,
//!         id_token: heapless::String::try_from("ABC123").unwrap(),
//!         r#type: IdTokenEnum::ISO14443,
//!     },
//!     iso15118_certificate_hash_data: None,
//! };
//!
//! let error = request.validate().unwrap_err();
//! assert_eq!(
//!     error.kind(),
//!     ValidationErrorKind::TooLong { len: 6000, max: 5500 },
//! );
//! # }
//! ```
//!
//! # What it does not check
//!
//! Only what the JSON schemas state. Cross-field rules from the
//! specification's prose (a `TransactionEventRequest` with
//! `eventType: Started` must carry a `triggerReason`, and so on) are out of
//! scope, as is anything about the caller's own `CustomDataType` payload or
//! 2.x's deliberately untyped `DataTransfer.data` -- the spec constrains
//! neither, so neither is visited. Validating those is the caller's, and
//! nothing stops a caller from implementing [`Validate`] for their own
//! payload type.
//!
//! Validation is never automatic: nothing on the serialize path calls it,
//! so a payload is checked exactly when you ask for it. Sending is the
//! natural place --- an over-long field comes back from the peer as a
//! `CALLERROR` you cannot correlate to a field, whereas
//! [`ValidationError`] names it.

use core::fmt;

/// How many path segments a [`ValidationError`] carries before it starts
/// truncating (see [`ValidationError::path_truncated`]).
///
/// The path is built as the error unwinds and is held inline, so this is
/// most of [`ValidationError`]'s size -- 296 bytes, which
/// `Result<(), ValidationError>` costs on the stack at every level of a
/// recursive `validate`. It is sized for the longest path OCPP can produce
/// rather than trimmed below that: array indices are segments too, so 2.1's
/// `chargingProfile.chargingSchedule[i].salesTariff.salesTariffEntry[j].`
/// `consumptionCost[k].cost[l].amount` is eleven of them, and truncating
/// real paths would blunt the errors exactly where they are hardest to
/// diagnose by hand.
pub const MAX_PATH_DEPTH: usize = 16;

/// A type that can be checked against the constraints its schema states.
///
/// Implemented for every generated message and nested type. The
/// implementation checks this type's own fields and recurses into nested
/// structs and arrays, so calling it on a message validates the whole
/// payload.
pub trait Validate {
    /// `Ok(())` if every schema constraint reachable from `self` holds, or
    /// the first violation found. Field order within a struct is the
    /// schema's, but which violation comes first is not otherwise a
    /// stable part of the API.
    fn validate(&self) -> Result<(), ValidationError>;
}

/// One step of the JSON path to a failing value: an object key, or a
/// position within an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSegment {
    /// A property name, as it appears on the wire (camelCase), not the
    /// Rust field name.
    Field(&'static str),
    /// A zero-based index into an array.
    Index(usize),
}

/// Which of OCPP's two constraint categories a violation falls into, and so
/// which `CALLERROR` code answers it.
///
/// Each version names these slightly differently -- 1.6J's
/// `OccurenceConstraintViolation` lost a letter that 2.x's
/// `OccurrenceConstraintViolation` restored -- so this is the version-
/// independent classification, and the caller picks the code from their
/// version's `RpcErrorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintClass {
    /// A value breaks its own field's rule: too long, out of range, not on
    /// the required step. Answered with `PropertyConstraintViolation`.
    Property,
    /// An array holds the wrong number of elements. Answered with
    /// `Occur(r)enceConstraintViolation`.
    Occurrence,
}

/// What was wrong with the value.
///
/// The numeric variants carry `f64` for uniformity across integer and
/// number fields; comparison itself is done in the field's own type, so an
/// integer bound is never decided in floating point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidationErrorKind {
    /// A string longer than its schema's `maxLength`, counted in
    /// characters.
    TooLong { len: usize, max: usize },
    /// An array longer than its schema's `maxItems`.
    TooManyItems { len: usize, max: usize },
    /// An array shorter than its schema's `minItems`.
    TooFewItems { len: usize, min: usize },
    /// A number below its schema's `minimum`.
    BelowMinimum { value: f64, min: f64 },
    /// A number above its schema's `maximum`.
    AboveMaximum { value: f64, max: f64 },
    /// A number that is not a whole multiple of its schema's `multipleOf`.
    NotMultipleOf { value: f64, multiple: f64 },
}

impl ValidationErrorKind {
    /// Which OCPP constraint category this violates, and so which
    /// `CALLERROR` code answers it. See [`ConstraintClass`].
    pub fn constraint_class(&self) -> ConstraintClass {
        match self {
            Self::TooManyItems { .. } | Self::TooFewItems { .. } => ConstraintClass::Occurrence,
            Self::TooLong { .. }
            | Self::BelowMinimum { .. }
            | Self::AboveMaximum { .. }
            | Self::NotMultipleOf { .. } => ConstraintClass::Property,
        }
    }
}

impl fmt::Display for ValidationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { len, max } => {
                write!(f, "expected at most {max} characters, got {len}")
            }
            Self::TooManyItems { len, max } => {
                write!(f, "expected at most {max} item{}, got {len}", plural(*max))
            }
            Self::TooFewItems { len, min } => {
                write!(f, "expected at least {min} item{}, got {len}", plural(*min))
            }
            Self::BelowMinimum { value, min } => write!(f, "expected at least {min}, got {value}"),
            Self::AboveMaximum { value, max } => write!(f, "expected at most {max}, got {value}"),
            Self::NotMultipleOf { value, multiple } => {
                write!(f, "expected a multiple of {multiple}, got {value}")
            }
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// A schema constraint that a payload broke, and the path to the value that
/// broke it.
///
/// Built innermost-first: the check that failed creates it with an empty
/// path, and each enclosing field or array index is prepended as the error
/// unwinds, so [`Display`](fmt::Display) prints a JSON path a peer's error
/// message can be matched against.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    path: heapless::Vec<PathSegment, MAX_PATH_DEPTH>,
    truncated: bool,
    kind: ValidationErrorKind,
}

impl ValidationError {
    /// A violation at the root, with no path yet. Prepend context with
    /// [`in_field`](Self::in_field) / [`in_index`](Self::in_index).
    pub fn new(kind: ValidationErrorKind) -> Self {
        Self {
            path: heapless::Vec::new(),
            truncated: false,
            kind,
        }
    }

    /// What was wrong with the value.
    pub fn kind(&self) -> ValidationErrorKind {
        self.kind
    }

    /// The path from the validated payload's root to the failing value,
    /// outermost segment first. Empty when the payload itself is what
    /// failed.
    pub fn path(&self) -> &[PathSegment] {
        &self.path
    }

    /// Whether the path lost its outermost segments to
    /// [`MAX_PATH_DEPTH`]. The innermost ones -- the part that says what
    /// actually failed -- are always kept.
    pub fn path_truncated(&self) -> bool {
        self.truncated
    }

    /// Records that this violation was found inside property `name`
    /// (its wire name, camelCase).
    pub fn in_field(self, name: &'static str) -> Self {
        self.prepend(PathSegment::Field(name))
    }

    /// Records that this violation was found at index `index` of the array
    /// currently outermost in the path.
    pub fn in_index(self, index: usize) -> Self {
        self.prepend(PathSegment::Index(index))
    }

    /// Dropping the segment rather than the error is the only option that
    /// keeps `validate` infallible in the presence of a pathological type
    /// graph, and dropping the *outermost* one loses the least: the
    /// segments nearest the failure are what identify it.
    fn prepend(mut self, segment: PathSegment) -> Self {
        if self.path.insert(0, segment).is_err() {
            self.truncated = true;
        }

        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.truncated {
            f.write_str("...")?;
        }

        if self.path.is_empty() && !self.truncated {
            // An empty path means the payload itself failed -- printing
            // nothing there would render as a bare `: expected ...`.
            f.write_str("<payload>")?;
        }

        for (position, segment) in self.path.iter().enumerate() {
            match segment {
                PathSegment::Field(name) => {
                    if position > 0 || self.truncated {
                        f.write_str(".")?;
                    }
                    f.write_str(name)?;
                }
                PathSegment::Index(index) => write!(f, "[{index}]")?,
            }
        }

        write!(f, ": {}", self.kind)
    }
}

impl core::error::Error for ValidationError {}

/// Rejects a string longer than `max` *characters*.
///
/// OCPP states `maxLength` in characters and Rust measures strings in
/// bytes, so this counts `char`s: a field bounded at 20 accepts twenty
/// non-ASCII characters, as the specification intends, even though they
/// occupy more than twenty bytes.
pub fn check_max_length(value: &str, max: usize) -> Result<(), ValidationError> {
    let len = value.chars().count();

    if len > max {
        return Err(ValidationError::new(ValidationErrorKind::TooLong {
            len,
            max,
        }));
    }

    Ok(())
}

/// Rejects an array shorter than its schema's `minItems`.
pub fn check_min_items(len: usize, min: usize) -> Result<(), ValidationError> {
    if len < min {
        return Err(ValidationError::new(ValidationErrorKind::TooFewItems {
            len,
            min,
        }));
    }

    Ok(())
}

/// Rejects an array longer than its schema's `maxItems`.
pub fn check_max_items(len: usize, max: usize) -> Result<(), ValidationError> {
    if len > max {
        return Err(ValidationError::new(ValidationErrorKind::TooManyItems {
            len,
            max,
        }));
    }

    Ok(())
}

/// Rejects an integer below its schema's `minimum`.
pub fn check_min_i64(value: i64, min: i64) -> Result<(), ValidationError> {
    if value < min {
        return Err(ValidationError::new(ValidationErrorKind::BelowMinimum {
            value: value as f64,
            min: min as f64,
        }));
    }

    Ok(())
}

/// Rejects an integer above its schema's `maximum`.
pub fn check_max_i64(value: i64, max: i64) -> Result<(), ValidationError> {
    if value > max {
        return Err(ValidationError::new(ValidationErrorKind::AboveMaximum {
            value: value as f64,
            max: max as f64,
        }));
    }

    Ok(())
}

/// Rejects a number below its schema's `minimum`.
///
/// A `NaN` compares false against every bound, so it passes here rather
/// than being reported as out of range -- it is not a number the schema has
/// anything to say about, and it cannot be serialized as JSON at all.
pub fn check_min_f64(value: f64, min: f64) -> Result<(), ValidationError> {
    if value < min {
        return Err(ValidationError::new(ValidationErrorKind::BelowMinimum {
            value,
            min,
        }));
    }

    Ok(())
}

/// Rejects a number above its schema's `maximum`. `NaN` passes, as in
/// [`check_min_f64`].
pub fn check_max_f64(value: f64, max: f64) -> Result<(), ValidationError> {
    if value > max {
        return Err(ValidationError::new(ValidationErrorKind::AboveMaximum {
            value,
            max,
        }));
    }

    Ok(())
}

/// How far `value / multiple` may sit from a whole number and still count
/// as a multiple, relative to the size of the quotient.
///
/// Every `multipleOf` in every OCPP version is 1.6J's `0.1` on charging
/// limits, and a tenth has no exact binary representation: `16.1 / 0.1` is
/// `160.99999999999997`, not `161`. An exact test would reject values that
/// arrived from the wire as conformant tenths. This tolerance is far larger
/// than the accumulated representation error and far smaller than the
/// smallest gap the spec cares about (half a step, `0.05`), so it separates
/// the two cleanly.
const MULTIPLE_OF_TOLERANCE: f64 = 1e-9;

/// The largest quotient this can decide. Beyond `2^53` consecutive integers
/// are no longer distinguishable in an `f64`, so "is this a whole number"
/// stops meaning anything; such a value is accepted rather than reported
/// against a test that cannot be performed. No OCPP field comes close.
const MULTIPLE_OF_MAX_QUOTIENT: f64 = 9_007_199_254_740_992.0;

/// Rejects a number that is not a whole multiple of `multiple`, within
/// [`MULTIPLE_OF_TOLERANCE`].
pub fn check_multiple_of(value: f64, multiple: f64) -> Result<(), ValidationError> {
    if multiple == 0.0 {
        return Ok(());
    }

    let quotient = value / multiple;
    let magnitude = abs(quotient);

    // `NaN` is not a value the schema has anything to say about, and it
    // cannot be serialized as JSON at all -- same reasoning as the
    // `minimum`/`maximum` checks above.
    if quotient.is_nan() || magnitude > MULTIPLE_OF_MAX_QUOTIENT {
        return Ok(());
    }

    // `as i64` truncates toward zero, so nudging by half a step in the
    // quotient's own direction rounds to nearest -- `round` itself is not
    // available in `core`.
    let nearest = if quotient >= 0.0 {
        (quotient + 0.5) as i64 as f64
    } else {
        (quotient - 0.5) as i64 as f64
    };

    let tolerance = MULTIPLE_OF_TOLERANCE * if magnitude > 1.0 { magnitude } else { 1.0 };

    if abs(quotient - nearest) > tolerance {
        return Err(ValidationError::new(ValidationErrorKind::NotMultipleOf {
            value,
            multiple,
        }));
    }

    Ok(())
}

/// `f64::abs` lives in `std`, and this crate is `no_std`.
fn abs(value: f64) -> f64 {
    if value < 0.0 { -value } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_within_its_limit_passes() {
        assert!(check_max_length("abc", 3).is_ok());
    }

    #[test]
    fn a_string_over_its_limit_reports_the_length_and_the_limit() {
        let error = check_max_length("abcd", 3).unwrap_err();

        assert_eq!(error.kind(), ValidationErrorKind::TooLong { len: 4, max: 3 });
    }

    /// OCPP states `maxLength` in characters, not bytes -- a 20-character
    /// name with an umlaut in it is 21 bytes and still conformant.
    #[test]
    fn max_length_counts_characters_not_bytes() {
        assert!(check_max_length("Ärger", 5).is_ok());
    }

    #[test]
    fn an_empty_array_fails_a_min_items_of_one() {
        let error = check_min_items(0, 1).unwrap_err();

        assert_eq!(error.kind(), ValidationErrorKind::TooFewItems { len: 0, min: 1 });
    }

    #[test]
    fn an_integer_below_its_minimum_is_rejected() {
        let error = check_min_i64(-1, 0).unwrap_err();

        assert_eq!(
            error.kind(),
            ValidationErrorKind::BelowMinimum { value: -1.0, min: 0.0 }
        );
    }

    /// 1.6J's charging limits are the only `multipleOf` in any version, and
    /// they are all `0.1` -- which is not representable in binary floating
    /// point, so an exact remainder test rejects values that came off the
    /// wire as valid tenths.
    #[test]
    fn a_tenth_is_a_multiple_of_one_tenth_despite_float_representation() {
        assert!(check_multiple_of(16.1, 0.1).is_ok());
        assert!(check_multiple_of(0.3, 0.1).is_ok());
        assert!(check_multiple_of(-2.5, 0.1).is_ok());
    }

    #[test]
    fn a_value_between_steps_is_not_a_multiple() {
        let error = check_multiple_of(16.15, 0.1).unwrap_err();

        assert_eq!(
            error.kind(),
            ValidationErrorKind::NotMultipleOf { value: 16.15, multiple: 0.1 }
        );
    }

    #[test]
    fn an_error_starts_with_an_empty_path() {
        let error = check_max_length("abcd", 3).unwrap_err();

        assert_eq!(error.path(), &[]);
    }

    #[test]
    fn nesting_an_error_builds_the_path_from_the_inside_out() {
        let error = check_min_items(0, 1)
            .unwrap_err()
            .in_field("chargingSchedulePeriod")
            .in_index(0)
            .in_field("chargingSchedule");

        assert_eq!(
            error.path(),
            &[
                PathSegment::Field("chargingSchedule"),
                PathSegment::Index(0),
                PathSegment::Field("chargingSchedulePeriod"),
            ]
        );
    }

    #[test]
    fn display_renders_the_path_in_json_terms() {
        let error = check_min_items(0, 1)
            .unwrap_err()
            .in_field("chargingSchedulePeriod")
            .in_index(0)
            .in_field("chargingSchedule");

        let mut rendered = heapless::String::<256>::new();
        core::fmt::write(&mut rendered, format_args!("{error}")).unwrap();

        assert_eq!(
            rendered.as_str(),
            "chargingSchedule[0].chargingSchedulePeriod: expected at least 1 item, got 0"
        );
    }

    #[test]
    fn display_of_a_root_level_error_says_so_rather_than_printing_an_empty_path() {
        let error = check_max_length("abcd", 3).unwrap_err();

        let mut rendered = heapless::String::<256>::new();
        core::fmt::write(&mut rendered, format_args!("{error}")).unwrap();

        assert_eq!(
            rendered.as_str(),
            "<payload>: expected at most 3 characters, got 4"
        );
    }

    /// The path is a fixed-capacity buffer, so a pathologically deep type
    /// graph has to degrade rather than panic or silently drop the *inner*
    /// segments that say what actually failed.
    #[test]
    fn a_path_deeper_than_the_buffer_is_marked_truncated_and_keeps_the_innermost_segments() {
        let mut error = check_max_length("abcd", 3).unwrap_err();
        for _ in 0..(MAX_PATH_DEPTH + 3) {
            error = error.in_field("nested");
        }

        assert!(error.path_truncated());
        assert_eq!(error.path().len(), MAX_PATH_DEPTH);

        let mut rendered = heapless::String::<512>::new();
        core::fmt::write(&mut rendered, format_args!("{error}")).unwrap();
        assert!(rendered.starts_with("..."), "{rendered}");
    }

    /// A CSMS answering a bad payload needs the OCPP error code that goes
    /// with it, and the spec splits these two ways: a value that breaks a
    /// field's own rule is a property constraint, a wrong *number* of
    /// elements is an occurrence constraint.
    #[test]
    fn error_kinds_classify_as_the_ocpp_constraint_they_violate() {
        assert_eq!(
            ValidationErrorKind::TooLong { len: 4, max: 3 }.constraint_class(),
            ConstraintClass::Property
        );
        assert_eq!(
            ValidationErrorKind::TooFewItems { len: 0, min: 1 }.constraint_class(),
            ConstraintClass::Occurrence
        );
        assert_eq!(
            ValidationErrorKind::TooManyItems { len: 9, max: 8 }.constraint_class(),
            ConstraintClass::Occurrence
        );
    }

    /// `ValidationError` is returned by value through every level of a
    /// recursive `validate`, so its size is a real cost on a target
    /// counting stack bytes -- and one that would otherwise grow silently
    /// as variants are added. See [`MAX_PATH_DEPTH`] for why the path is
    /// sized the way it is.
    #[test]
    fn the_error_stays_the_size_its_path_capacity_implies() {
        assert_eq!(core::mem::size_of::<PathSegment>(), 16);
        assert_eq!(core::mem::size_of::<ValidationError>(), 296);
    }

    #[test]
    fn validation_error_is_a_core_error() {
        fn assert_error<E: core::error::Error>() {}

        assert_error::<ValidationError>();
    }
}
