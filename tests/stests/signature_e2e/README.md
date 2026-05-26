# Signature End-to-End Tests

## Overview

This test suite verifies the complete signature preservation and verification flow in Ankaios:
- Ed25519 signature verification on workload manifests
- Signature preservation through the complete persistence lifecycle
- Server restart and state restoration with signature re-verification
- Security enforcement: tampering detection, counter rollback protection, and unsigned manifest rejection

## Purpose

These tests ensure that signed workload manifests:
1. Are verified when submitted to the server
2. Preserve their signatures through persistence
3. Are re-verified on server restart
4. Detect tampering attempts
5. Prevent replay attacks via monotonic counters

## Test Coverage

The test suite includes 8 comprehensive test scenarios:

1. **Signed Manifest Persistence** - Verifies that signed YAML is preserved byte-for-byte through the persistence plugin
2. **Server Restart and Restoration** - Complete lifecycle: sign → persist → restart → restore → re-verify
3. **Tampered Manifest Detection** - Security: filesystem tampering is detected via signature verification
4. **Unsigned Manifest Rejection** - Policy enforcement when `require_signature=true`
5. **Counter Rollback Prevention** - Monotonic counter validation prevents replay attacks
6. **Multiple Workload Signing** - Different workloads with different signature counters
7. **MQTT Workload Integration** - Real workload deployment with signature preservation
8. **Fleet Connector Integration** - Advanced multi-workload scenario with persistent state

## Prerequisites

### Build Requirements
```bash
# Build Ankaios binaries with signature support
cargo build --release --target x86_64-unknown-linux-gnu

# Build container images
podman build -t localhost/ank-persist:test -f examples/plugins/basic_persistency/Dockerfile .
podman build -t localhost/fleet-connector:test -f tools/tutorials/fleet_management/fleet-connector/Dockerfile tools/tutorials/fleet_management/fleet-connector
```

### Python Dependencies
```bash
# Install Robot Framework and cryptography library
pip3 install --user cryptography pyyaml robotframework
```

### Environment Variables
The tests use `ANKAIOS_TARGET` environment variable to locate binaries:
```bash
export ANKAIOS_TARGET=x86_64-unknown-linux-gnu
```

## Running Tests

### All Tests
```bash
robot --outputdir test-results tests/stests/signature_e2e/
```

### Specific Test
```bash
robot --test "Signed Manifest Is Persisted With Signature Block" \
     --outputdir test-results \
     tests/stests/signature_e2e/signature_persistence.robot
```

### By Tag
```bash
# Critical security tests only
robot --include critical --outputdir test-results tests/stests/signature_e2e/

# Security-focused tests
robot --include security --outputdir test-results tests/stests/signature_e2e/
```

## Test Output

### Success
```
Signed Manifest Is Persisted With Signature Block    | PASS |
✅ SUCCESS: Persistence file contains valid signature
```

### Failure
```
Signed Manifest Is Persisted With Signature Block    | FAIL |
Persistence file must contain signature field
```

Inspect `/tmp/ankaios-server.log` and `/tmp/ank-persist.log` for details.

## Test Structure

```
tests/stests/signature_e2e/
├── README.md                           # This file
└── signature_persistence.robot         # Main test suite (8 test cases)

tests/resources/
└── signature_utils.resource            # Robot Framework keywords and utilities
```

## Debugging

Test logs and state files are created in temporary directories. Check the test output for exact paths.

### Common Issues

**Missing container images:**
The tests require pre-built container images. Verify environment with:
```bash
podman images localhost/ank-persist:test
podman images localhost/fleet-connector:test
```

**Missing binaries:**
Ensure binaries are built for the correct target:
```bash
ls -la target/${ANKAIOS_TARGET}/release/ank-server
ls -la target/${ANKAIOS_TARGET}/release/ank
```

**Test failures:**
Check the Robot Framework logs in `test-results/` directory for detailed error messages.

## Implementation Details

**Signature Verification:** Server-side signature validation using Ed25519 cryptography  
**Persistence:** Per-workload signed YAML files with atomic writes  
**Security:** Constant-time verification, counter-based replay protection, path traversal prevention

For implementation details, see:
- `server/src/signature_validator.rs` - Ed25519 signature verification
- `examples/plugins/basic_persistency/` - Persistence plugin implementation
- `common/src/path_security.rs` - Path traversal protection
- `common/src/secure_io.rs` - Atomic file operations
