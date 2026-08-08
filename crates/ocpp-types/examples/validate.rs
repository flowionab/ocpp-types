//! Checking a payload against the spec limits the types can't carry.
//!
//! Run with: `cargo run --example validate -p ocpp-types --features validate`

use ocpp_types::v201::AuthorizeRequest;
use ocpp_types::v201::common::{IdToken, IdTokenEnum};
use ocpp_types::validate::{ConstraintClass, Validate};

fn main() {
    let mut request: AuthorizeRequest = AuthorizeRequest {
        certificate: None,
        custom_data: None,
        id_token: IdToken {
            additional_info: None,
            custom_data: None,
            // Bounded at `maxLength: 36` by the schema, so this is a
            // `heapless::String<36>` and an over-long value can't be built
            // in the first place -- nothing for `validate` to check.
            id_token: heapless::String::try_from("ABC123").unwrap(),
            r#type: IdTokenEnum::ISO14443,
        },
        iso15118_certificate_hash_data: None,
    };

    println!("conformant: {:?}", request.validate());

    // `certificate` is `maxLength: 5500` in the schema, but 5500 is too
    // large to inline as a `heapless::String`, so with `alloc` it is a
    // plain growable `String`. The type accepts this; the spec does not.
    request.certificate = Some("-".repeat(6000));

    match request.validate() {
        Ok(()) => println!("no violations"),
        Err(error) => {
            println!("rejected: {error}");
            println!("  kind:    {:?}", error.kind());
            println!("  path:    {:?}", error.path());

            // Which `CALLERROR` a CSMS answers with. The code's spelling is
            // version-specific (1.6J dropped an `r` from "occurrence"), so
            // the classification is what's shared.
            let code = match error.kind().constraint_class() {
                ConstraintClass::Property => "PropertyConstraintViolation",
                ConstraintClass::Occurrence => "OccurrenceConstraintViolation",
            };
            println!("  answer:  {code}");
        }
    }

    // `minItems: 1` is the other half: no collection type can say "at least
    // one", so an empty required array is only caught here.
    request.certificate = None;
    request.iso15118_certificate_hash_data = Some(heapless::Vec::new());

    if let Err(error) = request.validate() {
        println!("rejected: {error}");
    }
}
