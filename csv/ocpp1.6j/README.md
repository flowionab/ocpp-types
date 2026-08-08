# 1.6J spec tables

Value sets for fields the 1.6J JSON schemas type as bare strings, in the same
format as `csv/ocpp2.0.1/` and `csv/ocpp2.1/`. `ocpp-codegen` reads these via
`--csv` and generates `ocpp-types::v16::standard`.

## Provenance — read before trusting these

**Unlike the 2.0.1 and 2.1 tables, these were not machine-exported from a
specification appendix.** The OCA does not publish the 1.6 configuration key
or security event tables in any machine-readable form; they exist only as
prose tables in the OCPP 1.6 specification and the OCPP 1.6 Security
Whitepaper. These files were transcribed from those documents.

That makes them the least-verified data in this repo. Treat a mismatch
between one of these files and the specification as a bug in the file.

| File | Source |
| --- | --- |
| `configuration_keys.csv` | 1.6 spec, "Standard Configuration Key Names & Values"; Security Whitepaper (Ed. 2/3) for the `Security` profile rows |
| `security_events.csv` | Security Whitepaper (Ed. 2/3), security events table |

## Specific entries to verify against the documents

These are the transcriptions where the specification's exact spelling is
least certain, because the 1.6 Security Whitepaper was written alongside
OCPP 2.0 and mixes 1.6 terminology ("Charge Point", "Central System") with
2.0's ("Charging Station", "CSMS"). The wire value must match the document
exactly — a wrong spelling here silently fails against a real Central
System.

- `FailedToAuthenticateAtCsms` and `CsmsFailedToAuthenticate` keep 2.0's
  `Csms` spelling, on the assumption the whitepaper reused it rather than
  substituting `CentralSystem`.
- `InvalidCsmsCertificate` — same question; the alternative spelling would be
  `InvalidCentralSystemCertificate`.
- `InvalidChargePointCertificate` — transcribed with 1.6's "Charge Point"
  where 2.0.1 says `InvalidChargingStationCertificate`.
- `SettingSystemTime` — 2.0.1 qualifies this with a threshold variable that
  has no 1.6 equivalent, so the description here is shortened rather than
  translated.
- `DiscardedRenewedClientCertificate` is deliberately **absent**: it is an
  OCPP 2.0.1 addition tied to certificate-renewal flows 1.6 does not have.

Configuration keys are on firmer ground — they are stable and near-universally
implemented — with two notes:

- `AuthorizeRemoteTxRequests` is listed with accessibility `R or RW`, which
  is what the specification itself states rather than a transcription
  artefact.
- `AuthorizationKey` is effectively write-only (reading it back yields no
  value); the accessibility column says `RW` because that is what the
  specification's table says.

## Format

Same conventions as the 2.x tables: `;`-delimited, RFC 4180 quoting, first
row is the header. Column names are matched case-insensitively by
`crates/ocpp-codegen/src/standard.rs`, which is also where the column-to-
accessor mapping lives.

`Required` is read as a yes/no column and becomes `is_required()`. The 1.6
specification words this as "required"/"optional"; it is written here as
`Yes`/`No` to match the generator's yes/no handling.
