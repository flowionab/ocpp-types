# 1.6J schemas

Two different kinds of file live here.

## Vendored (authoritative)

The 56 files covering the Core, Firmware Management, Local Auth List
Management, Reservation, Smart Charging and Remote Trigger profiles come from
the OCA's official release, kept alongside as
`OCPP-1.6-JSON-Schemas.zip`. Those are authoritative; if one disagrees with
the zip, the zip wins.

## Authored (Security Whitepaper)

The 22 files for the 11 Security Whitepaper message pairs are **not
vendored** — the OCA never published JSON schemas for the whitepaper
messages, which exist only as prose and tables in the OCPP 1.6 Security
Whitepaper (Ed. 2/3). They were authored here so the generator has a single
source of truth to work from.

| Message pair | Basis |
| --- | --- |
| `CertificateSigned` | adapted from `schemas/ocpp2.0.1/CertificateSigned*` |
| `DeleteCertificate` | adapted from 2.0.1 |
| `GetInstalledCertificateIds` | adapted from 2.0.1 |
| `GetLog` | adapted from 2.0.1 |
| `InstallCertificate` | adapted from 2.0.1 |
| `LogStatusNotification` | adapted from 2.0.1 |
| `SecurityEventNotification` | adapted from 2.0.1 |
| `SignCertificate` | adapted from 2.0.1 |
| `ExtendedTriggerMessage` | whitepaper only — no 2.0.1 counterpart |
| `SignedUpdateFirmware` | whitepaper only — 2.0.1 folded this into `UpdateFirmware` |
| `SignedFirmwareStatusNotification` | whitepaper only — 2.0.1 folded this into `FirmwareStatusNotification` |

"Adapted" means the message shape, enum members and required-field sets were
taken from the 2.0.1 schema and then converted to 1.6 conventions: draft-04,
`id: urn:OCPP:1.6:2019:12:<Title>Request`, no `customData`, and 1.6's own
field names.

### Field bounds to verify

Message shapes and enum members are the parts most likely to be right; the
`maxLength` bounds are the parts most likely to be wrong, because the
whitepaper states them in a table that had to be transcribed. Every value in
`csv/ocpp1.6j/security_events.csv` fits its field comfortably, so a wrong
bound here will not show up as a test failure — it shows up as a
`heapless::String<N>` that is the wrong size for real traffic.

| Field | Bound used |
| --- | --- |
| `CertificateSigned.certificateChain` | 10000 |
| `InstallCertificate.certificate` | 5500 |
| `SignCertificate.csr` | 5500 |
| `SignedUpdateFirmware.firmware.signingCertificate` | 5500 |
| `SignedUpdateFirmware.firmware.signature` | 800 |
| `SignedUpdateFirmware.firmware.location` | 512 |
| `GetLog.log.remoteLocation` | 512 |
| `GetLogResponse.filename` | 255 |
| `SecurityEventNotification.type` | 255 |
| `SecurityEventNotification.techInfo` | 255 |
| `CertificateHashData.issuerNameHash` / `.issuerKeyHash` | 128 |
| `CertificateHashData.serialNumber` | 40 |

`SecurityEventNotification.type` is the one worth checking first: 2.0.1 bounds
the equivalent field at 50, and 255 is used here on the recollection that 1.6
was more generous. Erring high was deliberate — too small a bound rejects
conformant vendor-specific event names, which is the worse failure.

`GetInstalledCertificateIdsResponse.certificateHashData` is left with no
`maxItems`, matching the whitepaper, so it generates as a const-generic
capacity like every other unbounded field in the crate.

### Deviations from 1.6 house style

The vendored 1.6 schemas inline every nested object and use no `definitions`.
These files follow that, with one exception: `DeleteCertificate.json` and
`GetInstalledCertificateIdsResponse.json` both declare a
`CertificateHashDataType` definition and `$ref` it, so the generator emits a
single shared `CertificateHashData` type. Inlining it in both would produce
two structurally identical types (`CertificateHashData` and
`CertificateHashDataItem`) that callers would have to convert between to move
a hash from one message to the other.
