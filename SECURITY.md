# Security Policy

## Reporting a vulnerability

If you find a security issue in `ocpp-types` — for example, a way that a malformed or malicious
OCPP message (from a charge point, CSMS, or MITM) could cause a panic, memory-safety issue,
buffer overflow, or otherwise unsafe behavior when parsed by this crate — please report it
privately rather than opening a public issue.

- Preferred: use GitHub's [private vulnerability reporting](https://github.com/flowionab/ocpp-types/security/advisories/new)
  for this repository.
- Alternatively, email **joatin@granlund.io** with details and, if possible, a minimal
  reproduction.

Please include:

- The crate version and enabled features (`serde`, `alloc`) affected.
- A minimal reproduction (schema/message payload, or code snippet).
- The impact you'd expect (e.g. panic/DoS, memory unsafety, incorrect parsing that could be
  security-relevant downstream).

You should get an initial response within a few days. Once a fix is available, we'll publish a
patched release and, where applicable, a GitHub Security Advisory crediting the report.

## Supported versions

This crate is pre-1.0; only the latest published version on crates.io receives fixes.
