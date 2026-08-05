#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IdTag(heapless::String<20>);

impl IdTag {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for IdTag {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // `heapless` 0.9 returns `CapacityError` here where 0.8 returned
        // `()`. The only failure mode either way is "longer than the
        // wrapper's bound", which this `Error = ()` already conveys, so
        // discard it rather than leaking a `heapless` type into this
        // crate's public API.
        heapless::String::try_from(value).map(Self).map_err(|_| ())
    }
}
