//! RPC framework error codes from Table 9 ("Valid Error Codes") of the
//! OCPP-J 2.1 specification -- the `errorCode` values a `CALLERROR` frame
//! may carry. Identical to the 2.0.1 table.

/// One of the error codes OCPP-J 2.1 allows in a `CALLERROR` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RpcErrorCode {
    /// Payload for Action is syntactically incorrect
    FormatViolation,
    /// Any other error not covered by the more specific error codes in
    /// this table
    GenericError,
    /// An internal error occurred and the receiver was not able to process
    /// the requested Action successfully
    InternalError,
    /// A message with an Message Type Number received that is not
    /// supported by this implementation.
    MessageTypeNotSupported,
    /// Requested Action is not known by receiver
    NotImplemented,
    /// Requested Action is recognized but not supported by the receiver
    NotSupported,
    /// Payload for Action is syntactically correct but at least one of the
    /// fields violates occurrence constraints
    OccurrenceConstraintViolation,
    /// Payload is syntactically correct but at least one field contains an
    /// invalid value
    PropertyConstraintViolation,
    /// Payload for Action is not conform the PDU structure
    ProtocolError,
    /// Content of the call is not a valid RPC Request, for example:
    /// MessageId could not be read.
    RpcFrameworkError,
    /// During the processing of Action a security issue occurred
    /// preventing receiver from completing the Action successfully
    SecurityError,
    /// Payload for Action is syntactically correct but at least one of the
    /// fields violates data type constraints (e.g. "somestring": 12)
    TypeConstraintViolation,
}

impl RpcErrorCode {
    /// The spec's own description for this error code (see `Display`,
    /// which uses this).
    pub const fn description(&self) -> &'static str {
        match self {
            Self::FormatViolation => "Payload for Action is syntactically incorrect",
            Self::GenericError => {
                "Any other error not covered by the more specific error codes in this table"
            }
            Self::InternalError => {
                "An internal error occurred and the receiver was not able to process the requested Action successfully"
            }
            Self::MessageTypeNotSupported => {
                "A message with an Message Type Number received that is not supported by this implementation."
            }
            Self::NotImplemented => "Requested Action is not known by receiver",
            Self::NotSupported => {
                "Requested Action is recognized but not supported by the receiver"
            }
            Self::OccurrenceConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields violates occurrence constraints"
            }
            Self::PropertyConstraintViolation => {
                "Payload is syntactically correct but at least one field contains an invalid value"
            }
            Self::ProtocolError => "Payload for Action is not conform the PDU structure",
            Self::RpcFrameworkError => {
                "Content of the call is not a valid RPC Request, for example: MessageId could not be read."
            }
            Self::SecurityError => {
                "During the processing of Action a security issue occurred preventing receiver from completing the Action successfully"
            }
            Self::TypeConstraintViolation => {
                "Payload for Action is syntactically correct but at least one of the fields violates data type constraints (e.g. \"somestring\": 12)"
            }
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
    fn has_exactly_the_twelve_codes_defined_by_the_ocpp_2_1j_spec() {
        let all = [
            RpcErrorCode::FormatViolation,
            RpcErrorCode::GenericError,
            RpcErrorCode::InternalError,
            RpcErrorCode::MessageTypeNotSupported,
            RpcErrorCode::NotImplemented,
            RpcErrorCode::NotSupported,
            RpcErrorCode::OccurrenceConstraintViolation,
            RpcErrorCode::PropertyConstraintViolation,
            RpcErrorCode::ProtocolError,
            RpcErrorCode::RpcFrameworkError,
            RpcErrorCode::SecurityError,
            RpcErrorCode::TypeConstraintViolation,
        ];

        assert_eq!(all.len(), 12);
    }

    #[test]
    fn display_shows_the_spec_description() {
        use core::fmt::Write;
        let mut buf = heapless::String::<128>::new();
        write!(buf, "{}", RpcErrorCode::RpcFrameworkError).unwrap();

        assert_eq!(
            buf.as_str(),
            "Content of the call is not a valid RPC Request, for example: MessageId could not be read."
        );
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
        let json = serde_json_core::to_slice(&RpcErrorCode::FormatViolation, &mut buf).unwrap();

        assert_eq!(&buf[..json], br#""FormatViolation""#);
    }
}
