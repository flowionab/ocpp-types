//! RPC framework error codes from Table 7 ("Valid Error Codes") of the
//! OCPP-J 1.6 specification -- the `errorCode` values a `CALLERROR` frame
//! may carry.

/// One of the error codes OCPP-J 1.6 allows in a `CALLERROR` frame.
///
/// Note the spelling: `OccurenceConstraintViolation` is missing an "r".
/// That's the spec, not a typo here -- later versions correct it to
/// `OccurrenceConstraintViolation` (see `v201::RpcErrorCode`), so the two
/// are deliberately distinct variants across versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RpcErrorCode {
    /// Requested Action is not known by receiver
    NotImplemented,
    /// Requested Action is recognized but not supported by the receiver
    NotSupported,
    /// An internal error occurred and the receiver was not able to process
    /// the requested Action successfully
    InternalError,
    /// Payload for Action is incomplete
    ProtocolError,
    /// During the processing of Action a security issue occurred
    /// preventing receiver from completing the Action successfully
    SecurityError,
    /// Payload for Action is syntactically incorrect or not conform the
    /// PDU structure for Action
    FormationViolation,
    /// Payload is syntactically correct but at least one field contains an
    /// invalid value
    PropertyConstraintViolation,
    /// Payload for Action is syntactically correct but at least one of the
    /// fields violates occurence constraints
    OccurenceConstraintViolation,
    /// Payload for Action is syntactically correct but at least one of the
    /// fields violates data type constraints (e.g. "somestring": 12)
    TypeConstraintViolation,
    /// Any other error not covered by the previous ones
    GenericError,
}

impl RpcErrorCode {
    /// The spec's own description for this error code (see `Display`,
    /// which uses this).
    pub const fn description(&self) -> &'static str {
        match self {
            Self::NotImplemented => "Requested Action is not known by receiver",
            Self::NotSupported => {
                "Requested Action is recognized but not supported by the receiver"
            }
            Self::InternalError => {
                "An internal error occurred and the receiver was not able to process the requested Action successfully"
            }
            Self::ProtocolError => "Payload for Action is incomplete",
            Self::SecurityError => {
                "During the processing of Action a security issue occurred preventing receiver from completing the Action successfully"
            }
            Self::FormationViolation => {
                "Payload for Action is syntactically incorrect or not conform the PDU structure for Action"
            }
            Self::PropertyConstraintViolation => {
                "Payload is syntactically correct but at least one field contains an invalid value"
            }
            Self::OccurenceConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields violates occurence constraints"
            }
            Self::TypeConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields violates data type constraints (e.g. \"somestring\": 12)"
            }
            Self::GenericError => "Any other error not covered by the previous ones",
        }
    }
}

impl core::fmt::Display for RpcErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.description())
    }
}

impl core::error::Error for RpcErrorCode {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_exactly_the_ten_codes_defined_by_the_ocpp_1_6j_spec() {
        let all = [
            RpcErrorCode::NotImplemented,
            RpcErrorCode::NotSupported,
            RpcErrorCode::InternalError,
            RpcErrorCode::ProtocolError,
            RpcErrorCode::SecurityError,
            RpcErrorCode::FormationViolation,
            RpcErrorCode::PropertyConstraintViolation,
            RpcErrorCode::OccurenceConstraintViolation,
            RpcErrorCode::TypeConstraintViolation,
            RpcErrorCode::GenericError,
        ];

        assert_eq!(all.len(), 10);
    }

    #[test]
    fn display_shows_the_spec_description() {
        use core::fmt::Write;
        let mut buf = heapless::String::<128>::new();
        write!(buf, "{}", RpcErrorCode::NotImplemented).unwrap();

        assert_eq!(buf.as_str(), "Requested Action is not known by receiver");
    }

    #[test]
    fn implements_the_core_error_trait() {
        fn assert_error<T: core::error::Error>() {}
        assert_error::<RpcErrorCode>();
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes_using_the_exact_wire_spelling() {
        let mut buf = [0u8; 64];
        let json = serde_json_core::to_slice(&RpcErrorCode::OccurenceConstraintViolation, &mut buf)
            .unwrap();

        assert_eq!(&buf[..json], br#""OccurenceConstraintViolation""#);
    }
}
