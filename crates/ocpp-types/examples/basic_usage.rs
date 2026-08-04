//! Construct a message, read its OCPP action name, and inspect it.
//!
//! Run with: `cargo run --example basic_usage -p ocpp-types`

use ocpp_types::v16::BootNotificationRequest;
use ocpp_types::Action;

fn main() {
    let request = BootNotificationRequest {
        charge_point_vendor: "Flowion".try_into().expect("fits in 20 chars"),
        charge_point_model: "Simulator".try_into().expect("fits in 20 chars"),
        charge_box_serial_number: None,
        charge_point_serial_number: None,
        firmware_version: None,
        iccid: None,
        imsi: None,
        meter_serial_number: None,
        meter_type: None,
    };

    println!("action: {}", BootNotificationRequest::ACTION);
    println!("request: {request:#?}");

    // Fields bounded by the spec (`maxLength`) reject values that don't
    // fit, right at construction -- not somewhere downstream at the wire.
    let too_long = "a".repeat(21);
    let result: Result<heapless::String<20>, _> = too_long.as_str().try_into();
    assert!(result.is_err());
    println!("a 21-char vendor name is rejected: {}", result.is_err());
}
