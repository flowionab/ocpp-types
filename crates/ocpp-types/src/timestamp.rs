//! A fixed-size timestamp for OCPP's `dateTime` fields.
//!
//! Every OCPP version types its timestamps identically -- `{"type":
//! "string", "format": "date-time"}` -- and not one of the 142 such fields
//! across 1.6J, 2.0.1 and 2.1 declares a `maxLength`. Treated as an
//! unbounded string, each reserved the generator's 1024-byte default, which
//! is how `v16::HeartbeatResponse` came to be 1,032 bytes to carry a
//! 24-character instant.
//!
//! [`OcppTimestamp`] stores the instant decomposed instead: 16 bytes, no
//! allocator, and comparable, which a string is not.
//!
//! # Representation
//!
//! The instant is Unix seconds plus nanoseconds, with the UTC offset kept
//! alongside so a local-time timestamp survives a round trip in the form the
//! peer sent it. The offset is *presentation*, not identity: two values
//! naming the same instant in different offsets are equal and compare equal,
//! because they are the same moment. Use [`OcppTimestamp::utc_offset_minutes`]
//! when the written form matters.
//!
//! # Format
//!
//! Parsing accepts RFC 3339 as the specification requires, with `T`/`t`/` `
//! as the date-time separator and `Z`/`z` or `±HH:MM` as the offset.
//! Fractional seconds of any length are accepted and kept to nanosecond
//! precision.
//!
//! Writing emits a canonical RFC 3339 form: `Z` for UTC, `±HH:MM` otherwise,
//! and fractional seconds only when non-zero (trimmed to milliseconds when
//! they divide evenly, which is what OCPP deployments use in practice).

use core::fmt;

/// The widest string [`OcppTimestamp::to_rfc3339`] can produce:
/// `-262143-01-01T00:00:00.123456789+01:00`. Callers sizing their own buffer
/// should use this.
pub const MAX_RFC3339_LEN: usize = 40;

/// A `dateTime` value from any OCPP version.
///
/// See this module's documentation for the representation and its
/// equality semantics.
#[derive(Debug, Clone, Copy)]
pub struct OcppTimestamp {
    /// Seconds since the Unix epoch, UTC.
    secs: i64,
    /// Nanoseconds within the second, `0..1_000_000_000`.
    nanos: u32,
    /// Offset from UTC in minutes, as written by the peer.
    offset_minutes: i16,
}

/// Why a string is not an OCPP `dateTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimestampError {
    /// Not RFC 3339 shaped -- wrong length, wrong separators, or a
    /// non-digit where a digit belongs.
    Malformed,
    /// Shaped correctly but not a real instant, e.g. `2024-02-30` or an
    /// hour of 24.
    OutOfRange,
}

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "not an RFC 3339 date-time",
            Self::OutOfRange => "RFC 3339 date-time with an out-of-range field",
        })
    }
}

impl core::error::Error for TimestampError {}

/// Days from the Unix epoch to the given civil date, by Howard Hinnant's
/// `days_from_civil`. Valid for any proleptic Gregorian date; the era
/// arithmetic relies on truncating division, which is what Rust's `/` does.
const fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146097 + day_of_era - 719468
}

/// Inverse of [`days_from_civil`].
const fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let day_of_era = days - era * 146097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524
        - day_of_era / 146096)
        / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as u32;

    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl OcppTimestamp {
    /// The Unix epoch, `1970-01-01T00:00:00Z`.
    pub const UNIX_EPOCH: Self = Self {
        secs: 0,
        nanos: 0,
        offset_minutes: 0,
    };

    /// Builds a UTC timestamp from Unix seconds and nanoseconds.
    ///
    /// Returns [`TimestampError::OutOfRange`] if `nanos` is not a valid
    /// subsecond value.
    pub const fn from_unix(secs: i64, nanos: u32) -> Result<Self, TimestampError> {
        if nanos >= 1_000_000_000 {
            return Err(TimestampError::OutOfRange);
        }

        Ok(Self {
            secs,
            nanos,
            offset_minutes: 0,
        })
    }

    /// Seconds since the Unix epoch, UTC, regardless of the written offset.
    pub const fn unix_seconds(&self) -> i64 {
        self.secs
    }

    /// Nanoseconds within the second.
    pub const fn subsec_nanos(&self) -> u32 {
        self.nanos
    }

    /// The UTC offset this timestamp was written with, in minutes. Zero for
    /// a `Z` timestamp, and for anything built from Unix time.
    pub const fn utc_offset_minutes(&self) -> i16 {
        self.offset_minutes
    }

    /// Returns the same instant, to be written with `offset_minutes` instead.
    ///
    /// Returns [`TimestampError::OutOfRange`] for an offset beyond ±24 hours.
    pub const fn with_utc_offset_minutes(self, offset_minutes: i16) -> Result<Self, TimestampError> {
        if offset_minutes <= -1440 || offset_minutes >= 1440 {
            return Err(TimestampError::OutOfRange);
        }

        Ok(Self {
            offset_minutes,
            ..self
        })
    }

    /// Parses an RFC 3339 date-time, as every OCPP version's `dateTime`
    /// fields carry.
    pub fn parse_rfc3339(text: &str) -> Result<Self, TimestampError> {
        let bytes = text.as_bytes();

        // `YYYY-MM-DDTHH:MM:SS` is the shortest legal prefix, and an offset
        // (`Z` at minimum) is required by RFC 3339.
        if bytes.len() < 20 {
            return Err(TimestampError::Malformed);
        }

        let year = parse_number(&bytes[0..4])? as i64;
        expect(bytes[4], b'-')?;
        let month = parse_number(&bytes[5..7])?;
        expect(bytes[7], b'-')?;
        let day = parse_number(&bytes[8..10])?;

        match bytes[10] {
            b'T' | b't' | b' ' => {}
            _ => return Err(TimestampError::Malformed),
        }

        let hour = parse_number(&bytes[11..13])?;
        expect(bytes[13], b':')?;
        let minute = parse_number(&bytes[14..16])?;
        expect(bytes[16], b':')?;
        let second = parse_number(&bytes[17..19])?;

        let mut cursor = 19;
        let mut nanos = 0u32;

        if bytes[cursor] == b'.' || bytes[cursor] == b',' {
            cursor += 1;
            let start = cursor;

            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                // Only the first nine digits are representable; the rest are
                // still consumed so the offset parses from the right place.
                if cursor - start < 9 {
                    nanos = nanos * 10 + u32::from(bytes[cursor] - b'0');
                }
                cursor += 1;
            }

            let digits = cursor - start;

            if digits == 0 {
                return Err(TimestampError::Malformed);
            }

            // Scale a short fraction up to nanoseconds: `.5` is 500_000_000.
            for _ in digits.min(9)..9 {
                nanos *= 10;
            }
        }

        if cursor >= bytes.len() {
            return Err(TimestampError::Malformed);
        }

        let offset_minutes = match bytes[cursor] {
            b'Z' | b'z' if cursor + 1 == bytes.len() => 0i32,
            b'+' | b'-' => {
                if cursor + 6 != bytes.len() {
                    return Err(TimestampError::Malformed);
                }

                let sign = if bytes[cursor] == b'-' { -1i32 } else { 1 };
                let offset_hour = parse_number(&bytes[cursor + 1..cursor + 3])? as i32;
                expect(bytes[cursor + 3], b':')?;
                let offset_minute = parse_number(&bytes[cursor + 4..cursor + 6])? as i32;

                if offset_hour > 23 || offset_minute > 59 {
                    return Err(TimestampError::OutOfRange);
                }

                sign * (offset_hour * 60 + offset_minute)
            }
            _ => return Err(TimestampError::Malformed),
        };

        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return Err(TimestampError::OutOfRange);
        }

        // A leap second is reported as `:60`. There is no representation for
        // it, so it clamps to the last representable instant of the minute
        // rather than being rejected -- a peer may legitimately send one.
        if hour > 23 || minute > 59 || second > 60 {
            return Err(TimestampError::OutOfRange);
        }

        let second = second.min(59);
        let days = days_from_civil(year, month, day);
        let secs = days * 86_400
            + i64::from(hour) * 3600
            + i64::from(minute) * 60
            + i64::from(second)
            - i64::from(offset_minutes) * 60;

        Ok(Self {
            secs,
            nanos,
            offset_minutes: offset_minutes as i16,
        })
    }

    /// Writes this timestamp as RFC 3339 into `buf`, returning the written
    /// slice.
    ///
    /// `buf` must be at least [`MAX_RFC3339_LEN`] bytes; a shorter buffer
    /// yields `None` rather than a truncated timestamp.
    pub fn to_rfc3339<'buf>(&self, buf: &'buf mut [u8]) -> Option<&'buf str> {
        if buf.len() < MAX_RFC3339_LEN {
            return None;
        }

        // Render in the written offset, not UTC.
        let local = self.secs + i64::from(self.offset_minutes) * 60;
        let days = local.div_euclid(86_400);
        let time_of_day = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);

        let mut at = 0;

        write_year(buf, &mut at, year);
        buf[at] = b'-';
        at += 1;
        write_two(buf, &mut at, month as u64);
        buf[at] = b'-';
        at += 1;
        write_two(buf, &mut at, day as u64);
        buf[at] = b'T';
        at += 1;
        write_two(buf, &mut at, (time_of_day / 3600) as u64);
        buf[at] = b':';
        at += 1;
        write_two(buf, &mut at, (time_of_day % 3600 / 60) as u64);
        buf[at] = b':';
        at += 1;
        write_two(buf, &mut at, (time_of_day % 60) as u64);

        if self.nanos != 0 {
            buf[at] = b'.';
            at += 1;

            // Milliseconds when they divide evenly, which is what OCPP
            // deployments emit; full nanosecond precision otherwise.
            if self.nanos.is_multiple_of(1_000_000) {
                write_padded(buf, &mut at, u64::from(self.nanos / 1_000_000), 3);
            } else {
                write_padded(buf, &mut at, u64::from(self.nanos), 9);
            }
        }

        if self.offset_minutes == 0 {
            buf[at] = b'Z';
            at += 1;
        } else {
            let (sign, magnitude) = if self.offset_minutes < 0 {
                (b'-', -i32::from(self.offset_minutes))
            } else {
                (b'+', i32::from(self.offset_minutes))
            };

            buf[at] = sign;
            at += 1;
            write_two(buf, &mut at, (magnitude / 60) as u64);
            buf[at] = b':';
            at += 1;
            write_two(buf, &mut at, (magnitude % 60) as u64);
        }

        core::str::from_utf8(&buf[..at]).ok()
    }
}

fn expect(actual: u8, expected: u8) -> Result<(), TimestampError> {
    if actual == expected {
        Ok(())
    } else {
        Err(TimestampError::Malformed)
    }
}

fn parse_number(bytes: &[u8]) -> Result<u32, TimestampError> {
    let mut value = 0u32;

    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(TimestampError::Malformed);
        }

        value = value * 10 + u32::from(byte - b'0');
    }

    Ok(value)
}

fn write_two(buf: &mut [u8], at: &mut usize, value: u64) {
    write_padded(buf, at, value, 2);
}

fn write_padded(buf: &mut [u8], at: &mut usize, value: u64, width: usize) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    let mut value = value;

    while value > 0 {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }

    for _ in count..width {
        buf[*at] = b'0';
        *at += 1;
    }

    for index in (0..count).rev() {
        buf[*at] = digits[index];
        *at += 1;
    }
}

fn write_year(buf: &mut [u8], at: &mut usize, year: i64) {
    if year < 0 {
        buf[*at] = b'-';
        *at += 1;
        write_padded(buf, at, year.unsigned_abs(), 4);
    } else {
        write_padded(buf, at, year as u64, 4);
    }
}

impl fmt::Display for OcppTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = [0u8; MAX_RFC3339_LEN];

        match self.to_rfc3339(&mut buf) {
            Some(text) => f.write_str(text),
            None => Err(fmt::Error),
        }
    }
}

impl core::str::FromStr for OcppTimestamp {
    type Err = TimestampError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_rfc3339(text)
    }
}

impl TryFrom<&str> for OcppTimestamp {
    type Error = TimestampError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::parse_rfc3339(text)
    }
}

// Identity is the instant, not the way it was written: `2024-01-01T00:00:00Z`
// and `2024-01-01T01:00:00+01:00` are the same moment. Comparing the written
// offset too would make `Ord` disagree with what a reader means by "earlier".
impl PartialEq for OcppTimestamp {
    fn eq(&self, other: &Self) -> bool {
        self.secs == other.secs && self.nanos == other.nanos
    }
}

impl Eq for OcppTimestamp {}

impl core::hash::Hash for OcppTimestamp {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.secs.hash(state);
        self.nanos.hash(state);
    }
}

impl PartialOrd for OcppTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OcppTimestamp {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.secs
            .cmp(&other.secs)
            .then(self.nanos.cmp(&other.nanos))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for OcppTimestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; MAX_RFC3339_LEN];
        let text = self
            .to_rfc3339(&mut buf)
            .ok_or_else(|| serde::ser::Error::custom("timestamp is not representable"))?;

        serializer.serialize_str(text)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for OcppTimestamp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'v> serde::de::Visitor<'v> for Visitor {
            type Value = OcppTimestamp;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an RFC 3339 date-time")
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
                OcppTimestamp::parse_rfc3339(text).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

/// Conversions to and from [`chrono`], for callers that already use it.
///
/// Deliberately an optional feature rather than the crate's representation:
/// chrono's own serde support formats through an allocating `to_rfc3339`,
/// which the no-`alloc` build -- the one these sizes matter most for -- cannot
/// use. Keeping chrono at the boundary gives its ergonomics to callers who
/// want them without putting an allocator on the path of everyone else.
#[cfg(feature = "chrono")]
mod chrono_interop {
    use super::OcppTimestamp;
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};

    impl From<OcppTimestamp> for DateTime<Utc> {
        fn from(value: OcppTimestamp) -> Self {
            Utc.timestamp_opt(value.unix_seconds(), value.subsec_nanos())
                .single()
                .expect("an OcppTimestamp always names exactly one UTC instant")
        }
    }

    impl From<DateTime<Utc>> for OcppTimestamp {
        fn from(value: DateTime<Utc>) -> Self {
            OcppTimestamp::from_unix(value.timestamp(), value.timestamp_subsec_nanos())
                .expect("chrono keeps subsec nanos below one second")
        }
    }

    impl From<DateTime<FixedOffset>> for OcppTimestamp {
        fn from(value: DateTime<FixedOffset>) -> Self {
            let offset_minutes = (value.offset().local_minus_utc() / 60) as i16;

            OcppTimestamp::from_unix(value.timestamp(), value.timestamp_subsec_nanos())
                .expect("chrono keeps subsec nanos below one second")
                .with_utc_offset_minutes(offset_minutes)
                .expect("chrono offsets are within +/- 24 hours")
        }
    }

    impl From<OcppTimestamp> for DateTime<FixedOffset> {
        fn from(value: OcppTimestamp) -> Self {
            let offset = FixedOffset::east_opt(i32::from(value.utc_offset_minutes()) * 60)
                .expect("an OcppTimestamp offset is within +/- 24 hours");

            DateTime::<Utc>::from(value).with_timezone(&offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(text: &str) -> heapless::String<MAX_RFC3339_LEN> {
        let parsed = OcppTimestamp::parse_rfc3339(text).expect(text);
        let mut buf = [0u8; MAX_RFC3339_LEN];

        heapless::String::try_from(parsed.to_rfc3339(&mut buf).unwrap()).unwrap()
    }

    /// Anchors against instants whose Unix time is independently known, so a
    /// wrong civil-date algorithm can't pass by being self-consistent.
    #[test]
    fn parses_known_instants_to_their_known_unix_time() {
        for (text, secs) in [
            ("1970-01-01T00:00:00Z", 0),
            ("2000-03-01T00:00:00Z", 951_868_800),
            ("2024-02-29T12:00:00Z", 1_709_208_000),
            ("2038-01-19T03:14:07Z", 2_147_483_647),
            ("1969-12-31T23:59:59Z", -1),
            ("1900-01-01T00:00:00Z", -2_208_988_800),
        ] {
            let parsed = OcppTimestamp::parse_rfc3339(text).expect(text);
            assert_eq!(parsed.unix_seconds(), secs, "for {text}");
        }
    }

    #[test]
    fn round_trips_every_shape_the_spec_allows() {
        for text in [
            "2024-01-01T00:00:00Z",
            "2024-06-15T12:34:56Z",
            "2024-06-15T12:34:56.789Z",
            "1970-01-01T00:00:00Z",
            "2038-01-19T03:14:07Z",
        ] {
            assert_eq!(rendered(text).as_str(), text);
        }
    }

    #[test]
    fn accepts_the_separators_and_cases_rfc3339_permits() {
        let canonical = OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap();

        for text in [
            "2024-01-01t00:00:00Z",
            "2024-01-01 00:00:00Z",
            "2024-01-01T00:00:00z",
        ] {
            assert_eq!(OcppTimestamp::parse_rfc3339(text).unwrap(), canonical, "for {text}");
        }
    }

    #[test]
    fn an_offset_names_the_same_instant_as_its_utc_form() {
        let with_offset = OcppTimestamp::parse_rfc3339("2024-01-01T01:00:00+01:00").unwrap();
        let utc = OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap();

        assert_eq!(with_offset, utc);
        assert_eq!(with_offset.unix_seconds(), utc.unix_seconds());
        // ...but the written form is preserved, not normalized to UTC.
        assert_eq!(with_offset.utc_offset_minutes(), 60);
        assert_eq!(rendered("2024-01-01T01:00:00+01:00").as_str(), "2024-01-01T01:00:00+01:00");
        assert_eq!(rendered("2023-12-31T19:00:00-05:00").as_str(), "2023-12-31T19:00:00-05:00");
        assert_eq!(rendered("2024-01-01T00:30:00+05:30").as_str(), "2024-01-01T00:30:00+05:30");
    }

    /// A fraction is scaled by its digit count, not read as an integer:
    /// `.5` is 500ms, not 5ns.
    #[test]
    fn scales_fractional_seconds_by_their_length() {
        for (text, nanos) in [
            ("2024-01-01T00:00:00.5Z", 500_000_000),
            ("2024-01-01T00:00:00.05Z", 50_000_000),
            ("2024-01-01T00:00:00.123Z", 123_000_000),
            ("2024-01-01T00:00:00.000001Z", 1_000),
            ("2024-01-01T00:00:00.123456789Z", 123_456_789),
        ] {
            assert_eq!(
                OcppTimestamp::parse_rfc3339(text).unwrap().subsec_nanos(),
                nanos,
                "for {text}"
            );
        }

        // More than nanosecond precision is truncated, not rejected, and the
        // offset after it still parses.
        let over = OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00.1234567891Z").unwrap();
        assert_eq!(over.subsec_nanos(), 123_456_789);
    }

    #[test]
    fn writes_milliseconds_when_they_divide_evenly_and_nanoseconds_otherwise() {
        assert_eq!(rendered("2024-01-01T00:00:00.250Z").as_str(), "2024-01-01T00:00:00.250Z");
        assert_eq!(
            rendered("2024-01-01T00:00:00.123456789Z").as_str(),
            "2024-01-01T00:00:00.123456789Z"
        );
        // A zero fraction is omitted entirely.
        assert_eq!(rendered("2024-01-01T00:00:00.000Z").as_str(), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn rejects_dates_that_do_not_exist() {
        for text in [
            "2023-02-29T00:00:00Z", // 2023 is not a leap year
            "1900-02-29T00:00:00Z", // divisible by 100, not by 400
            "2024-13-01T00:00:00Z",
            "2024-04-31T00:00:00Z",
            "2024-01-01T24:00:00Z",
            "2024-01-01T00:60:00Z",
            "2024-01-01T00:00:00+24:00",
        ] {
            assert_eq!(
                OcppTimestamp::parse_rfc3339(text),
                Err(TimestampError::OutOfRange),
                "for {text}"
            );
        }

        // ...but 2000 and 2024 *are* leap years.
        assert!(OcppTimestamp::parse_rfc3339("2000-02-29T00:00:00Z").is_ok());
        assert!(OcppTimestamp::parse_rfc3339("2024-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn rejects_strings_that_are_not_rfc3339() {
        for text in [
            "",
            "2024-01-01",
            "2024-01-01T00:00:00",       // no offset
            "2024/01/01T00:00:00Z",
            "2024-01-01T00:00:00.Z",     // empty fraction
            "2024-01-01T00:00:00+0100",  // offset needs a colon
            "2024-01-01T00:00:00Zextra",
            "not-a-date-at-all!!!",
        ] {
            assert_eq!(
                OcppTimestamp::parse_rfc3339(text),
                Err(TimestampError::Malformed),
                "for {text}"
            );
        }
    }

    /// A leap second is a real thing a peer may send; there is no instant to
    /// map it to, so it clamps rather than failing the whole message.
    #[test]
    fn clamps_a_leap_second_rather_than_rejecting_it() {
        let parsed = OcppTimestamp::parse_rfc3339("2016-12-31T23:59:60Z").unwrap();

        assert_eq!(
            parsed,
            OcppTimestamp::parse_rfc3339("2016-12-31T23:59:59Z").unwrap()
        );
    }

    #[test]
    fn orders_by_instant_regardless_of_written_offset() {
        let earlier = OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap();
        let later = OcppTimestamp::parse_rfc3339("2024-01-01T00:00:01Z").unwrap();
        let same_instant_other_offset =
            OcppTimestamp::parse_rfc3339("2024-01-01T01:00:00+01:00").unwrap();

        assert!(earlier < later);
        assert_eq!(earlier.cmp(&same_instant_other_offset), core::cmp::Ordering::Equal);
    }

    #[test]
    fn round_trips_across_a_wide_span_of_days() {
        // Every 37th day for ~200 years, to exercise leap years, century
        // rules and the pre-epoch branch of the civil-date algorithm.
        let mut day = -36_500i64;

        while day < 36_500 {
            let stamp = OcppTimestamp::from_unix(day * 86_400 + 3661, 0).unwrap();
            let mut buf = [0u8; MAX_RFC3339_LEN];
            let text = stamp.to_rfc3339(&mut buf).unwrap();

            assert_eq!(
                OcppTimestamp::parse_rfc3339(text).unwrap(),
                stamp,
                "failed to round-trip {text}"
            );

            day += 37;
        }
    }

    #[test]
    fn to_rfc3339_refuses_a_buffer_it_could_overflow() {
        let stamp = OcppTimestamp::UNIX_EPOCH;
        let mut small = [0u8; MAX_RFC3339_LEN - 1];

        assert_eq!(stamp.to_rfc3339(&mut small), None);
    }

    #[test]
    fn is_sixteen_bytes() {
        // The whole point: a `dateTime` field used to reserve 1024.
        assert_eq!(core::mem::size_of::<OcppTimestamp>(), 16);
    }

    // --- OcppTimeOfDay / OcppDate ---------------------------------------

    /// Rendering needs no buffer, but the tests need something to compare.
    fn rendered_civil(value: &impl fmt::Display) -> heapless::String<16> {
        let mut out = heapless::String::new();
        core::fmt::Write::write_fmt(&mut out, format_args!("{value}")).unwrap();
        out
    }

    #[test]
    fn the_civil_types_are_as_small_as_their_fields_allow() {
        // `HH:MM` has 1,440 distinct values, so a minute resolution needs
        // more than a byte; two is the floor.
        assert_eq!(core::mem::size_of::<OcppTimeOfDay>(), 2);
        assert_eq!(core::mem::size_of::<OcppDate>(), 4);
    }

    #[test]
    fn time_of_day_round_trips_the_specs_format() {
        for text in ["00:00", "09:05", "13:45", "23:59"] {
            let parsed = OcppTimeOfDay::parse(text).expect(text);
            assert_eq!(rendered_civil(&parsed).as_str(), text);
        }
    }

    #[test]
    fn time_of_day_rejects_impossible_and_malformed_values() {
        for text in ["24:00", "23:60", "99:99"] {
            assert_eq!(OcppTimeOfDay::parse(text), Err(TimestampError::OutOfRange), "{text}");
        }

        // The spec's regex requires leading zeros and exactly `HH:MM`.
        for text in ["", "9:05", "09:5", "09:05:00", "0905", "ab:cd", "09-05"] {
            assert_eq!(OcppTimeOfDay::parse(text), Err(TimestampError::Malformed), "{text}");
        }
    }

    #[test]
    fn time_of_day_orders_chronologically() {
        let early = OcppTimeOfDay::parse("09:05").unwrap();
        let late = OcppTimeOfDay::parse("09:30").unwrap();

        assert!(early < late);
        assert!(OcppTimeOfDay::MIDNIGHT < early);
        assert_eq!(late.minutes_since_midnight(), 570);
    }

    #[test]
    fn date_round_trips_the_specs_format() {
        for text in ["1970-01-01", "2015-12-24", "2024-02-29", "9999-12-31"] {
            let parsed = OcppDate::parse(text).expect(text);
            assert_eq!(rendered_civil(&parsed).as_str(), text);
        }
    }

    #[test]
    fn date_rejects_days_the_month_does_not_have() {
        for text in ["2023-02-29", "1900-02-29", "2024-04-31", "2024-13-01", "2024-00-10"] {
            assert_eq!(OcppDate::parse(text), Err(TimestampError::OutOfRange), "{text}");
        }

        assert!(OcppDate::parse("2000-02-29").is_ok());
        assert!(OcppDate::parse("2024-02-29").is_ok());
    }

    #[test]
    fn date_orders_chronologically_and_converts_to_epoch_days() {
        let earlier = OcppDate::parse("2024-01-31").unwrap();
        let later = OcppDate::parse("2024-02-01").unwrap();

        assert!(earlier < later);
        assert_eq!(OcppDate::parse("1970-01-01").unwrap().days_from_epoch(), 0);
        assert_eq!(later.days_from_epoch() - earlier.days_from_epoch(), 1);
    }

}

#[cfg(all(test, feature = "chrono"))]
mod chrono_tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, Utc};

    #[test]
    fn round_trips_through_chrono_utc() {
        let ours = OcppTimestamp::parse_rfc3339("2024-06-15T12:34:56.789Z").unwrap();
        let theirs: DateTime<Utc> = ours.into();

        assert_eq!(theirs.timestamp(), ours.unix_seconds());
        assert_eq!(theirs.timestamp_subsec_nanos(), ours.subsec_nanos());
        assert_eq!(OcppTimestamp::from(theirs), ours);
    }

    /// The written offset is what a fixed-offset conversion must preserve --
    /// converting through UTC and back would silently rewrite the peer's
    /// local time as `Z`.
    #[test]
    fn a_fixed_offset_survives_the_round_trip() {
        let ours = OcppTimestamp::parse_rfc3339("2024-01-01T01:00:00+01:00").unwrap();
        let theirs: DateTime<FixedOffset> = ours.into();

        assert_eq!(theirs.offset().local_minus_utc(), 3600);

        let back = OcppTimestamp::from(theirs);
        assert_eq!(back, ours);
        assert_eq!(back.utc_offset_minutes(), 60);
    }

    #[test]
    fn chrono_agrees_with_our_parser_on_a_span_of_instants() {
        let mut day = -20_000i64;

        while day < 20_000 {
            let ours = OcppTimestamp::from_unix(day * 86_400 + 7261, 0).unwrap();
            let mut buf = [0u8; MAX_RFC3339_LEN];
            let text = ours.to_rfc3339(&mut buf).unwrap();

            let theirs = DateTime::parse_from_rfc3339(text)
                .unwrap_or_else(|e| panic!("chrono rejected our output {text}: {e}"));

            assert_eq!(theirs.timestamp(), ours.unix_seconds(), "for {text}");
            day += 61;
        }
    }
}

/// A local time of day, `HH:MM`.
///
/// 2.1's tariff conditions carry `startTimeOfDay`/`endTimeOfDay`, whose
/// property descriptions state the format (and its regex) while the schema
/// itself declares no `maxLength` -- so as strings they took the generator's
/// unbounded default.
///
/// Two bytes: an hour and a minute. One would not do, since a minute
/// resolution has 1,440 distinct values and a byte holds 256.
///
/// Ordering is chronological within the day, which is what the derive gives:
/// the fields are declared most-significant first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OcppTimeOfDay {
    hour: u8,
    minute: u8,
}

impl OcppTimeOfDay {
    /// Midnight, `00:00`.
    pub const MIDNIGHT: Self = Self { hour: 0, minute: 0 };

    /// Builds a time of day, rejecting an hour past 23 or a minute past 59.
    pub const fn new(hour: u8, minute: u8) -> Result<Self, TimestampError> {
        if hour > 23 || minute > 59 {
            return Err(TimestampError::OutOfRange);
        }

        Ok(Self { hour, minute })
    }

    pub const fn hour(&self) -> u8 {
        self.hour
    }

    pub const fn minute(&self) -> u8 {
        self.minute
    }

    /// Minutes since midnight, for arithmetic against a window.
    pub const fn minutes_since_midnight(&self) -> u16 {
        self.hour as u16 * 60 + self.minute as u16
    }

    /// Parses `HH:MM`, as the specification's own regex describes. Leading
    /// zeros are required, matching that regex.
    pub fn parse(text: &str) -> Result<Self, TimestampError> {
        let bytes = text.as_bytes();

        if bytes.len() != 5 {
            return Err(TimestampError::Malformed);
        }

        let hour = parse_number(&bytes[0..2])?;
        expect(bytes[2], b':')?;
        let minute = parse_number(&bytes[3..5])?;

        Self::new(hour as u8, minute as u8)
    }
}

impl fmt::Display for OcppTimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

impl core::str::FromStr for OcppTimeOfDay {
    type Err = TimestampError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<&str> for OcppTimeOfDay {
    type Error = TimestampError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::parse(text)
    }
}

/// A local calendar date, `YYYY-MM-DD`.
///
/// The companion to [`OcppTimeOfDay`]: 2.1's `validFromDate`/`validToDate`
/// state this format in prose and leave the property unbounded.
///
/// Four bytes, and chronologically ordered by the derive since the fields
/// run most-significant first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OcppDate {
    year: u16,
    month: u8,
    day: u8,
}

impl OcppDate {
    /// Builds a date, rejecting a month outside 1..=12 or a day the month
    /// doesn't have -- including 29 February in a common year.
    pub const fn new(year: u16, month: u8, day: u8) -> Result<Self, TimestampError> {
        if month < 1 || month > 12 || day < 1 || day > days_in_month(year as i64, month as u32) as u8
        {
            return Err(TimestampError::OutOfRange);
        }

        Ok(Self { year, month, day })
    }

    pub const fn year(&self) -> u16 {
        self.year
    }

    pub const fn month(&self) -> u8 {
        self.month
    }

    pub const fn day(&self) -> u8 {
        self.day
    }

    /// Days since the Unix epoch, for arithmetic against other dates.
    pub const fn days_from_epoch(&self) -> i64 {
        days_from_civil(self.year as i64, self.month as u32, self.day as u32)
    }

    /// Parses `YYYY-MM-DD`, as the specification's own regex describes.
    pub fn parse(text: &str) -> Result<Self, TimestampError> {
        let bytes = text.as_bytes();

        if bytes.len() != 10 {
            return Err(TimestampError::Malformed);
        }

        let year = parse_number(&bytes[0..4])?;
        expect(bytes[4], b'-')?;
        let month = parse_number(&bytes[5..7])?;
        expect(bytes[7], b'-')?;
        let day = parse_number(&bytes[8..10])?;

        Self::new(year as u16, month as u8, day as u8)
    }
}

impl fmt::Display for OcppDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl core::str::FromStr for OcppDate {
    type Err = TimestampError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl TryFrom<&str> for OcppDate {
    type Error = TimestampError;

    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::parse(text)
    }
}

/// Shared by [`OcppTimeOfDay`] and [`OcppDate`]: both render short, fixed
/// forms, so `Display` is enough and no buffer needs threading through.
#[cfg(feature = "serde")]
macro_rules! serde_via_display {
    ($ty:ty, $expecting:literal) => {
        #[cfg(feature = "serde")]
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'v> serde::de::Visitor<'v> for Visitor {
                    type Value = $ty;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str($expecting)
                    }

                    fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
                        <$ty>::parse(text).map_err(serde::de::Error::custom)
                    }
                }

                deserializer.deserialize_str(Visitor)
            }
        }
    };
}

#[cfg(feature = "serde")]
serde_via_display!(OcppTimeOfDay, "a time of day as HH:MM");
#[cfg(feature = "serde")]
serde_via_display!(OcppDate, "a date as YYYY-MM-DD");
