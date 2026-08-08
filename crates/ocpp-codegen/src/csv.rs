//! Reader for the semicolon-delimited spec tables under `csv/`.
//!
//! These tables carry the parts of the OCPP specification the JSON schemas
//! leave as bare `string` fields: standardized component and variable
//! names, security event types, reason codes, units of measure, and so on.
//! Every value set here corresponds to a schema property typed only as
//! `{"type": "string", "maxLength": N}` -- the schemas never enumerate
//! them, so without these tables the names are unavailable to the
//! generator.
//!
//! They come out of the spec appendices as spreadsheet exports, and arrive
//! with a spreadsheet export's quirks: `;` delimiters, RFC 4180 quoting
//! with `""` escapes, headers padded with stray spaces (`Name ; DataType`),
//! at least one description containing the delimiter itself, embedded tabs,
//! and sometimes no trailing newline. Rather than take a `csv` crate
//! dependency for a handful of small files read once at dev time, this
//! handles exactly those cases.

/// A parsed spec table: its header row plus one entry per data row.
///
/// Rows are *not* padded or truncated to the header width -- a short row
/// stays short, so callers reading an absent trailing column get `None`
/// rather than an empty string that looks like a real value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Index of the column titled `header`, compared case-insensitively so
    /// callers don't have to reproduce each export's capitalization. Header
    /// padding is already stripped by [`parse`].
    pub fn column(&self, header: &str) -> Option<usize> {
        self.headers
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(header))
    }

    /// The value of `row`'s `header` column, or `None` if the table has no
    /// such column or this row stops short of it. An empty cell reads as
    /// `None` too: in these exports a blank means "not stated", never a
    /// meaningful empty value.
    pub fn get<'a>(&self, row: &'a [String], header: &str) -> Option<&'a str> {
        let value = row.get(self.column(header)?)?.as_str();

        (!value.is_empty()).then_some(value)
    }
}

/// Parses a spec table. Blank lines are dropped, every field is trimmed,
/// and the first surviving line becomes [`Table::headers`].
pub fn parse(content: &str) -> Table {
    let mut records = content
        .strip_prefix('\u{feff}')
        .unwrap_or(content)
        .lines()
        .peekable();

    let mut rows = Vec::new();
    let mut headers = Vec::new();

    while let Some(line) = records.next() {
        // A quoted field may legitimately span lines; join continuations
        // before splitting so an embedded newline doesn't fake a new row.
        let mut record = line.to_string();
        while record.matches('"').count() % 2 == 1 {
            match records.next() {
                Some(next) => {
                    record.push('\n');
                    record.push_str(next);
                }
                None => break,
            }
        }

        let fields = split_record(&record);

        // A row with no content in any cell is a spacer, not data.
        // `components.csv` uses a bare `;` line to separate the controller
        // components from the physical ones, which a raw-line emptiness
        // check lets through as a row of empty strings.
        if fields.iter().all(String::is_empty) {
            continue;
        }

        if headers.is_empty() {
            headers = fields;
        } else {
            rows.push(fields);
        }
    }

    Table { headers, rows }
}

/// Splits one record on `;`, honouring RFC 4180 double quotes: a delimiter
/// inside quotes is data, and a doubled `""` is a literal quote.
fn split_record(record: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = record.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                chars.next();
                current.push('"');
            }
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    fields.push(current.trim().to_string());

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_semicolons_and_strips_the_padding_the_exports_carry() {
        // Real header from `csv/ocpp2.1/variables.csv`.
        let table = parse("Name ; DataType ; Unit ; Description\nACCurrent;decimal;A ; RMS AC Current\n");

        assert_eq!(table.headers, ["Name", "DataType", "Unit", "Description"]);
        assert_eq!(table.rows, [["ACCurrent", "decimal", "A", "RMS AC Current"]]);
    }

    /// `csv/ocpp2.1/dm_components_vars.csv` has exactly one row whose
    /// description contains the delimiter. Splitting naively shifts every
    /// later column on that row, which silently corrupts one entry.
    #[test]
    fn keeps_a_quoted_field_containing_the_delimiter_in_one_piece() {
        let table = parse(
            "Component;Variable;Description\n\
             DCDERCtrlr;MaxChargeRateVA;\"Maximum apparent power charge rating in voltamperes; may differ\"\n",
        );

        assert_eq!(
            table.rows,
            [[
                "DCDERCtrlr",
                "MaxChargeRateVA",
                "Maximum apparent power charge rating in voltamperes; may differ",
            ]]
        );
    }

    #[test]
    fn unescapes_a_doubled_quote_inside_a_quoted_field() {
        // Real row from `csv/ocpp2.1/dm_components_vars.csv`.
        let table = parse("Component;Variable;Description\nCPPWMController;State;\"IEC 61851-1 states (\"\"A\"\" to \"\"E\"\")\"\n");

        assert_eq!(
            table.get(&table.rows[0], "Description"),
            Some(r#"IEC 61851-1 states ("A" to "E")"#)
        );
    }

    /// Several of the supplied files end without a newline
    /// (`connectorenumtype.csv`, `paymentbrand.csv`, `reason_codes.csv`, ...),
    /// which drops the last value if the parser keys off line terminators.
    #[test]
    fn reads_the_final_row_when_the_file_has_no_trailing_newline() {
        let table = parse("Value;Description\nA;Amperes\nV;Volts");

        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1], ["V", "Volts"]);
    }

    #[test]
    fn skips_blank_lines_rather_than_emitting_empty_rows() {
        let table = parse("Value;Description\n\nA;Amperes\n   \nV;Volts\n");

        assert_eq!(table.rows.len(), 2);
    }

    /// `csv/ocpp2.1/components.csv` separates the controller components
    /// from the physical ones with a bare `;` line. Checking the raw line
    /// for emptiness lets that through as a row of empty strings, which
    /// then generates a nameless enum variant.
    #[test]
    fn skips_a_delimiters_only_spacer_row() {
        let table = parse("Component;Description\nWebPaymentsCtrlr;QR codes\n;\nAccessBarrier;Gates\n");

        assert_eq!(
            table.rows,
            [["WebPaymentsCtrlr", "QR codes"], ["AccessBarrier", "Gates"]]
        );
    }

    #[test]
    fn handles_crlf_line_endings() {
        let table = parse("Value;Description\r\nA;Amperes\r\n");

        assert_eq!(table.headers, ["Value", "Description"]);
        assert_eq!(table.rows, [["A", "Amperes"]]);
    }

    #[test]
    fn strips_a_utf8_byte_order_mark_from_the_first_header() {
        let table = parse("\u{feff}Value;Description\nA;Amperes\n");

        assert_eq!(table.column("Value"), Some(0));
    }

    #[test]
    fn column_lookup_ignores_case() {
        let table = parse("Security Event;Description;Critical\nFirmwareUpdated;Updated;Yes\n");

        assert_eq!(table.column("security event"), Some(0));
        assert_eq!(table.column("Critical"), Some(2));
        assert_eq!(table.column("Nonexistent"), None);
    }

    /// A blank cell in these exports means "not stated" -- e.g. a variable
    /// with no unit. Reading it as `Some("")` would put an empty unit into
    /// generated metadata.
    #[test]
    fn an_empty_cell_reads_as_absent() {
        let table = parse("Name;DataType;Unit\nACPhaseSwitchingSupported;boolean;\n");

        assert_eq!(table.get(&table.rows[0], "DataType"), Some("boolean"));
        assert_eq!(table.get(&table.rows[0], "Unit"), None);
    }

    /// `reason_codes.csv` uses a short row as a group heading (`Charging
    /// Profiles;;;`), so rows narrower than the header are normal input.
    #[test]
    fn a_row_shorter_than_the_header_reads_absent_for_the_missing_columns() {
        let table = parse("Group;Reason code;Description\nCharging Profiles\n");

        assert_eq!(table.get(&table.rows[0], "Group"), Some("Charging Profiles"));
        assert_eq!(table.get(&table.rows[0], "Reason code"), None);
    }

    #[test]
    fn joins_a_quoted_field_that_spans_more_than_one_line() {
        let table = parse("Value;Description\nA;\"first line\nsecond line\"\n");

        assert_eq!(table.rows.len(), 1);
        assert_eq!(
            table.get(&table.rows[0], "Description"),
            Some("first line\nsecond line")
        );
    }
}
