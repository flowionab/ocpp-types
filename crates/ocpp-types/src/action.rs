pub trait Action {
    const ACTION: &'static str;

    /// Serializes this message as JSON into `buf`, returning the written
    /// slice. No allocation: the caller owns and sizes the buffer, same as
    /// every other bounded type in this crate.
    #[cfg(feature = "serde")]
    fn to_json_slice<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<&'buf [u8], serde_json_core::ser::Error>
    where
        Self: serde::Serialize,
    {
        let len = serde_json_core::to_slice(self, buf)?;
        Ok(&buf[..len])
    }

    /// Same as [`Action::to_json_slice`], but returns the written bytes as
    /// `&str`. JSON emitted by `serde-json-core` is always valid UTF-8.
    #[cfg(feature = "serde")]
    fn to_json_str<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<&'buf str, serde_json_core::ser::Error>
    where
        Self: serde::Serialize,
    {
        let bytes = self.to_json_slice(buf)?;
        Ok(core::str::from_utf8(bytes).expect("serde-json-core always emits valid UTF-8"))
    }

    /// Deserializes this message from JSON bytes. No allocation: parsing
    /// borrows from `data` only transiently: `Self` owns everything via
    /// `heapless` collections, so nothing outlives this call.
    #[cfg(feature = "serde")]
    fn from_json_slice(data: &[u8]) -> Result<Self, serde_json_core::de::Error>
    where
        Self: Sized + serde::de::DeserializeOwned,
    {
        serde_json_core::from_slice(data).map(|(value, _remainder)| value)
    }

    /// Same as [`Action::from_json_slice`], but takes a `&str` instead of
    /// raw bytes (skipping the UTF-8 check `from_json_slice` would
    /// otherwise need, since `&str` already guarantees it).
    #[cfg(feature = "serde")]
    fn from_json_str(data: &str) -> Result<Self, serde_json_core::de::Error>
    where
        Self: Sized + serde::de::DeserializeOwned,
    {
        serde_json_core::from_str(data).map(|(value, _remainder)| value)
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct SampleRequest {
        #[serde(rename = "idTag")]
        id_tag: heapless::String<20>,
    }

    impl Action for SampleRequest {
        const ACTION: &'static str = "Sample";
    }

    fn sample() -> SampleRequest {
        SampleRequest {
            id_tag: heapless::String::try_from("ABC123").unwrap(),
        }
    }

    #[test]
    fn to_json_slice_writes_json_into_the_given_buffer() {
        let mut buf = [0u8; 64];
        let json = sample().to_json_slice(&mut buf).unwrap();

        assert_eq!(json, br#"{"idTag":"ABC123"}"#);
    }

    #[test]
    fn to_json_slice_errors_when_the_buffer_is_too_small() {
        let mut buf = [0u8; 4];

        assert!(sample().to_json_slice(&mut buf).is_err());
    }

    #[test]
    fn to_json_str_writes_json_into_the_given_buffer_as_utf8() {
        let mut buf = [0u8; 64];
        let json = sample().to_json_str(&mut buf).unwrap();

        assert_eq!(json, r#"{"idTag":"ABC123"}"#);
    }

    #[test]
    fn from_json_slice_parses_bytes_back_into_the_struct() {
        let parsed = SampleRequest::from_json_slice(br#"{"idTag":"ABC123"}"#).unwrap();

        assert_eq!(parsed, sample());
    }

    #[test]
    fn from_json_str_parses_a_str_back_into_the_struct() {
        let parsed = SampleRequest::from_json_str(r#"{"idTag":"ABC123"}"#).unwrap();

        assert_eq!(parsed, sample());
    }

    #[test]
    fn round_trips_through_bytes() {
        let mut buf = [0u8; 64];
        let json = sample().to_json_slice(&mut buf).unwrap();
        let parsed = SampleRequest::from_json_slice(json).unwrap();

        assert_eq!(parsed, sample());
    }
}
