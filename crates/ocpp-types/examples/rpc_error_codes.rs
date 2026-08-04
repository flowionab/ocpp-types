//! Per-version `RpcErrorCode`s for OCPP-J `CALLERROR` frames.
//!
//! Run with: `cargo run --example rpc_error_codes -p ocpp-types`

use ocpp_types::v16::RpcErrorCode as V16Error;
use ocpp_types::v201::RpcErrorCode as V201Error;

fn main() {
    let error = V16Error::NotImplemented;
    println!("1.6J: {error} ({error:?})");

    // 2.0.1/2.1 renamed 1.6J's `FormationViolation` to `FormatViolation`,
    // fixed a spelling error in `Occur(r)enceConstraintViolation`, and
    // added two new codes -- so each version's `RpcErrorCode` is its own
    // type, not shared.
    let error = V201Error::FormatViolation;
    println!("2.0.1: {error} ({error:?})");

    // `RpcErrorCode` implements `core::error::Error`, so it composes with
    // ordinary Rust error handling.
    fn handle(code: V16Error) -> Result<(), Box<dyn core::error::Error>> {
        Err(Box::new(code))
    }

    if let Err(err) = handle(V16Error::SecurityError) {
        println!("as a boxed error: {err}");
    }
}
