//! Size budgets for the message types.
//!
//! This crate's stated purpose is embedded, allocation-free OCPP, which makes
//! `size_of` a correctness property rather than a nicety: a message that
//! cannot be stack-allocated on the target cannot be used there at all,
//! however correct its fields are. Nothing else in the suite would notice a
//! type quietly growing to megabytes, because it still compiles, still
//! round-trips, and still passes every behavioural test.
//!
//! There are two tests here, doing different jobs.
//!
//! [`does_not_exceed_the_recorded_sizes`] is a **ratchet**. It runs in CI and
//! passes today: every type is asserted against its currently-measured size,
//! so a schema or generator change that inflates a type fails immediately.
//! When remediation shrinks a type, lower its number here.
//!
//! [`fits_the_mcu_budget`] is the **target**, and it fails today — by six
//! orders of magnitude in places. It is `#[ignore]`d so CI stays green while
//! the work is outstanding; run it to see the remaining gap:
//!
//! ```sh
//! cargo test -p ocpp-types --no-default-features -- --ignored fits_the_mcu_budget
//! cargo test -p ocpp-types -- --ignored fits_the_mcu_budget
//! ```
//!
//! # Reading the numbers
//!
//! Recorded sizes were measured on a 64-bit host. A 32-bit target (the actual
//! MCU) is the same or smaller, since the only width-dependent members are
//! pointers and `alloc` collections, so asserting `<=` holds on both.
//!
//! The no-`alloc` column is the one that matters for the crate's headline use
//! case, and it is the worse of the two: without `alloc`, every unbounded
//! field becomes a `heapless` collection sized by a const generic whose
//! default is 1024 (strings) or 16 (arrays), and those defaults multiply
//! through by-value nesting rather than adding. See
//! `crates/ocpp-codegen/src/schema.rs`.

/// The size a single message may occupy on an MCU.
///
/// A charging station on a 64–256 KB part needs room for several messages in
/// flight, TLS buffers, and its own state. 4 KB per message is generous
/// against that, and every message here already fits it under `alloc` except
/// the charging-profile family and `AuthorizeRequest`.
const MESSAGE_BUDGET: usize = 4 * 1024;

/// `CustomData` is the single highest-leverage type in the crate: it hangs off
/// nearly every struct at every level of nesting, so its size is multiplied by
/// the whole type graph rather than added once. Its `vendorId` is capped at
/// 255 by the specification, but no deployment needs anything like that.
const CUSTOM_DATA_BUDGET: usize = 64;

/// The measured size of each type, as `no_alloc / alloc`.
///
/// A curated list, not exhaustive -- and that is a known weakness. It covers
/// the hot-path messages that must stay small, the charging-profile family
/// that is the known problem, the shared leaf types that drive both, and the
/// largest types from a full sweep of all 387 message types.
///
/// The sweep is worth repeating rather than trusting this list: the first
/// version of it missed five of the eight biggest types, including
/// `v21::AuthorizeResponse` at 227 KB -- the reply to a message whose request
/// side had been carefully shrunk. Enumerate every type and re-measure before
/// concluding a size problem is solved.
macro_rules! measured_types {
    ($assertion:ident) => {
        $assertion! {
            // --- 1.6J -------------------------------------------------
            // Small and mostly flat, but not immune: `MeterValuesRequest`
            // nests three unbounded arrays, so the vec default cubes.
            v16::AuthorizeRequest                  =>          32 /     32,
            v16::BootNotificationRequest           =>         408 /    408,
            v16::HeartbeatResponse                 =>        16 /     16,
            v16::StatusNotificationRequest         =>        456 /    456,
            v16::StartTransactionRequest           =>        80 /     80,
            v16::MeterValuesRequest                =>      66784 /     48,
            v16::SetChargingProfileRequest         =>        424 /    184,
            v16::common::ChargingProfile           =>        416 /    176,

            // --- 2.0.1 ------------------------------------------------
            v201::BootNotificationRequest          =>        320 /   320,
            v201::HeartbeatResponse                =>        24 /    24,
            // A hot-path message: sent for every authorization. Its size
            // comes from `IdToken.additionalInfo`, not from charging
            // profiles, so it needs fixing independently of them.
            v201::AuthorizeRequest                 =>       5488 /  3520,
            v201::TransactionEventRequest          =>     147424 /   304,
            v201::SetChargingProfileRequest        =>    12368 /   752,
            v201::ReportChargingProfilesRequest    =>   98848 /    48,
            v201::RequestStartTransactionRequest   =>    14448 /   928,
            v201::NotifyEVChargingScheduleRequest  =>     4104 /   232,
            v201::common::CustomData               =>         264 /    264,
            v201::common::ChargingProfile          =>    12352 /   736,
            v201::common::ChargingSchedule         =>     4072 /    200,
            v201::common::ChargingSchedulePeriod   =>         56 /    56,

            // --- 2.1 --------------------------------------------------
            v21::BootNotificationRequest           =>        320 /   320,
            v21::HeartbeatResponse                 =>        24 /    24,
            v21::AuthorizeRequest                  =>       9512 /  1784,
            v21::TransactionEventRequest           =>     155472 /  3624,
            v21::SetChargingProfileRequest         =>    66312 /  11256,
            v21::ReportChargingProfilesRequest     =>  530432 /    80,
            v21::RequestStartTransactionRequest    =>    72344 /  11928,
            v21::NotifyEVChargingScheduleRequest   =>    21992 /  3640,
            v21::common::CustomData                =>         264 /    264,
            v21::common::ChargingProfile           =>    66296 /  11240,
            v21::common::ChargingSchedule          =>    21944 /  3592,
            // 2.1 only: two `Option<Vec<V2X*Point, 20>>`, each point
            // carrying its own `Option<CustomData>`. Spec-bounded, so this
            // layer is honest -- it is the by-value nesting above it that
            // turns 12 KB into 80 MB.
            v21::common::ChargingSchedulePeriod    =>       672 /  304,
            // --- worst offenders across all 387 message types -------
            // Added after a full sweep: the curated list above missed five of
            // the eight largest types, including a *response* to a hot-path
            // message. Nothing was watching them.
            v21::AuthorizeResponse                 =>      28416 /   4192,
            v21::ChangeTransactionTariffRequest    =>      24192 /   3712,
            v21::SetDefaultTariffRequest           =>      24152 /   3672,
            v21::NotifyChargingLimitRequest        =>      175632 /     88,
            v21::MeterValuesRequest                =>      147224 /     40,
            v201::MeterValuesRequest               =>      146200 /     40,
            v16::StopTransactionRequest            =>       66848 /    104,
            v21::SendLocalListRequest              =>       58144 /     40,
            v201::SendLocalListRequest             =>       22432 /     40,
            v201::NotifyReportRequest              =>       44984 /     64,
            v201::AuthorizeResponse                =>        1768 /    760,
            v16::AuthorizeResponse                 =>          72 /     72,

        }
    };
}

macro_rules! assert_ratchet {
    ($( $ty:ty => $no_alloc:literal / $alloc:literal ),* $(,)?) => {
        $({
            let recorded = if cfg!(feature = "alloc") { $alloc } else { $no_alloc };
            let actual = core::mem::size_of::<$ty>();

            assert!(
                actual <= recorded,
                "{} grew to {} bytes, over its recorded {}. If this is intended, \
                 update the number in `measured_types!` and say why in the changelog.",
                stringify!($ty),
                actual,
                recorded
            );
        })*
    };
}

macro_rules! assert_budget {
    ($( $ty:ty => $no_alloc:literal / $alloc:literal ),* $(,)?) => {
        let mut total = 0usize;
        let mut over = 0usize;
        let mut worst = 0usize;
        let mut worst_name = "";

        $({
            let _ = ($no_alloc, $alloc);
            let actual = core::mem::size_of::<$ty>();
            total += 1;

            if actual > MESSAGE_BUDGET {
                over += 1;

                if actual > worst {
                    worst = actual;
                    worst_name = stringify!($ty);
                }
            }
        })*

        assert_eq!(
            over, 0,
            "{} of {} measured types exceed the {} byte budget. Worst: {} at {} bytes \
             ({}x over). This is the remediation target, not a regression -- see the \
             crate's size roadmap.",
            over,
            total,
            MESSAGE_BUDGET,
            worst_name,
            worst,
            worst / MESSAGE_BUDGET
        );
    };
}

use crate::{v16, v201, v21};

/// Ratchet: nothing may grow beyond what is recorded above.
#[test]
fn does_not_exceed_the_recorded_sizes() {
    measured_types!(assert_ratchet);
}

/// Target: every message should fit an MCU's per-message budget.
///
/// Fails today. Ignored so it does not block CI while the remediation is
/// outstanding; see this module's docs for how to run it.
#[test]
#[ignore = "size remediation is outstanding; this is the target, not a regression"]
fn fits_the_mcu_budget() {
    measured_types!(assert_budget);
}

/// `CustomData`'s own budget, separate because it is a multiplier rather than
/// a message: every byte here is paid once per nested struct in the graph.
///
/// Fails today (264 bytes against 64).
#[test]
#[ignore = "size remediation is outstanding; this is the target, not a regression"]
fn custom_data_fits_its_budget() {
    for (name, actual) in [
        ("v201::common::CustomData", core::mem::size_of::<v201::common::CustomData>()),
        ("v21::common::CustomData", core::mem::size_of::<v21::common::CustomData>()),
    ] {
        assert!(
            actual <= CUSTOM_DATA_BUDGET,
            "{name} is {actual} bytes, over the {CUSTOM_DATA_BUDGET} byte budget"
        );
    }
}

/// The no-`alloc` path is the crate's headline use case, so its regression is
/// worth stating as its own fact rather than leaving implicit in the table:
/// several types are *larger* without `alloc` than with it, which is the
/// opposite of what "allocation-free" should cost.
///
/// Passes today -- it asserts the problem exists, and will need deleting once
/// the remediation lands. That is deliberate: it fails loudly when the premise
/// stops holding, rather than silently going stale.
#[test]
#[cfg(not(feature = "alloc"))]
fn documents_that_the_no_alloc_path_is_currently_the_larger_one() {
    // Measured under `alloc` (see `measured_types!`); hard-coded because this
    // build cannot instantiate the `alloc` variants to compare against.
    const CHARGING_PROFILE_UNDER_ALLOC: usize = 11240;

    // 1,416x before large spec-bounded arrays became caller-chosen
    // capacities, 21x after, 9x once the array default dropped to 8, and 4x
    // now that `customData` is a parameter. Ratchet this down as the
    // remaining work lands -- when it reaches 1, delete this test and the
    // ignored budgets along with it.
    const GAP: usize = 3;

    assert!(
        core::mem::size_of::<v21::common::ChargingProfile>()
            > CHARGING_PROFILE_UNDER_ALLOC * GAP,
        "the no-alloc ChargingProfile is no longer >{GAP}x its alloc counterpart -- \
         lower `GAP`, or if the size work has landed, delete this test along with \
         the ignored budgets"
    );
}
