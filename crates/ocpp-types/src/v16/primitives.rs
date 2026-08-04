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
        heapless::String::try_from(value).map(Self)
    }
}
