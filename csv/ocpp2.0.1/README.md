# 2.0.1 spec tables

Value sets for fields the 2.0.1 JSON schemas type as bare strings. Read by
`ocpp-codegen --csv` to generate `ocpp-types::v201::standard`.

## These files were partly 2.1 exports

As originally supplied, four of the six files here were **byte-identical to
their `csv/ocpp2.1/` counterparts** and carried 2.1-only content:
`components.csv`, `dm_components_vars.csv`, `security_events.csv` and
`units_of_measure.csv`. Only `variables.csv` and `reason_codes.csv` had been
curated per version.

`components.csv` and `dm_components_vars.csv` have since been narrowed, using
evidence rather than recollection. `security_events.csv` and
`units_of_measure.csv` were left alone — see below.

### Removed from `components.csv`

Each of these belongs to a feature whose **messages exist only in 2.1's
schemas**, and the schemas are authoritative:

| Component | Evidence |
| --- | --- |
| `ACDERCtrlr`, `DCDERCtrlr` | `Get`/`Set`/`Clear`/`ReportDERControl`, `NotifyDERAlarm`, `NotifyDERStartStop` are 2.1-only |
| `BatterySwapCtrlr`, `BatteryCartridge` | `BatterySwap`, `RequestBatterySwap` are 2.1-only |
| `WebPaymentsCtrlr` | `NotifyWebPaymentStarted` is 2.1-only |
| `V2XChargingCtrlr` | `AFRRSignal` is 2.1-only |
| `PaymentCtrlr` | `NotifySettlement`, `VatNumberValidation` are 2.1-only |

### Removed from `dm_components_vars.csv`

- All rows belonging to the components above (77 rows).
- 22 rows whose variable appears in `csv/ocpp2.1/variables.csv` but not in
  `csv/ocpp2.0.1/variables.csv`. That pair of files *was* curated per version,
  so the difference between them is evidence about which variables 2.1 added —
  e.g. `MaxPeriodicEventStreams` on `MonitoringCtrlr`, the `Supports*` family
  on `SmartChargingCtrlr`, `UpstreamInterval`/`UpstreamMeasurands` on the data
  controllers.

The filter deliberately does **not** drop the ~100 variables that are missing
from `variables.csv` in *both* versions (`Apn`, `OcppCsmsUrl`, `VpnServer`,
`WebSocketPingInterval`, ...). Those are absent from the dedicated table in
2.1 too, so their absence says nothing about the version they belong to.

`crates/ocpp-types/src/standard_test.rs` asserts none of the removed names
reappear, so a future re-export that reintroduces 2.1 data fails the suite
rather than shipping.

## Still unverified

- **`security_events.csv` and `units_of_measure.csv`** are still byte-identical
  to 2.1's. That may well be correct — neither list obviously changed between
  versions — but it has not been confirmed, and it is equally consistent with
  them being 2.1 exports nobody differentiated. Left as supplied rather than
  guessed at.
- **`FrequencySimulator` and `DataCollector`** appear only in
  `dm_components_vars.csv`, never in `components.csv`. Both look like 2.1
  additions (frequency simulation is tied to 2.1's `LocalFrequency` operation
  mode), but no 2.1-only message names them, so there was no evidence to act
  on and they were kept.
- The removals above are a **narrowing, not a verified 2.0.1 export.** A
  genuine re-export of all four files from the 2.0.1 appendices is still the
  only way to be confident nothing else is left over.
