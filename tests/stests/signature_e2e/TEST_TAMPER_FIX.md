# Fix for Test 3: Tampered Persistence File Is Rejected On Restore

## Problem Analysis

The test was failing because it used **truncation** to tamper with the .pb file:
- Truncating the last 10 bytes corrupts the **protobuf wire format** itself
- `UpdateStateRequest::decode()` fails in the plugin (line 996-1004 of `basic_persistency/src/main.rs`)
- Plugin logs "Failed to decode protobuf" and `continue`s, **never sending the request to the server**
- Server never gets a chance to verify the signature
- Test fails because server logs don't contain "signature verification failed"

## Root Cause

```rust
// Plugin code (line 996-1004)
match UpdateStateRequest::decode(&bytes[..]) {
    Ok(req) => { ... }
    Err(e) => {
        log::error!("Failed to decode protobuf {:?}: {}", workload_file, e);
        continue;  // ❌ Skips sending to server!
    }
}
```

The truncation approach corrupts the protobuf structure, causing decode to fail **before** signature verification.

## Solution

Instead of truncating the entire file (which breaks protobuf decode), we now:

1. Parse the protobuf wire format
2. Locate the signature bytes field within SignatureMetadata
3. Flip bits in the signature (XOR with 0xFF)
4. Re-encode with the same structure

This ensures:
- ✅ Protobuf structure remains **valid** (decode succeeds)
- ✅ Signature bytes are **corrupted** (verification fails)
- ✅ Plugin sends the request to the server
- ✅ Server runs signature verification and logs "signature verification failed"

## Implementation

### New File: `tamper_signature.py`

A Python script that uses low-level protobuf wire format manipulation to tamper with the signature field without breaking the protobuf structure.

**Key features:**
- No dependency on generated protobuf code (works standalone)
- Parses protobuf wire format to find Field 3 (signature_metadata)
- Within signature_metadata, finds Field 1 (signature bytes)
- Flips first 8 bytes of signature (XOR 0xFF)
- Re-encodes with same structure

### Test Update

**Old approach (broken):**
```robot
# TAMPER: Corrupt the binary protobuf file by truncating it
${original}=    Get Binary File    ${WORKLOADS_DIR}/nginx-persistent.pb
${length}=    Get Length    ${original}
${truncated_length}=    Evaluate    ${length} - 10
${tampered}=    Evaluate    bytes($original[:$truncated_length])    modules=builtins
Create Binary File    ${WORKLOADS_DIR}/nginx-persistent.pb    ${tampered}
```

**New approach (correct):**
```robot
# TAMPER: Corrupt the signature bytes within the protobuf structure
${result}=    Run Process    python3    ${CURDIR}/tamper_signature.py    ${WORKLOADS_DIR}/nginx-persistent.pb
Should Be Equal As Integers    ${result.rc}    0    msg=Tampering script failed: ${result.stderr}
```

## Expected Test Flow (After Fix)

1. Apply signed workload (nginx-persistent) with counter=20
2. Verify .pb file exists
3. Stop server
4. **Run tamper_signature.py** - corrupts signature but keeps protobuf valid
5. Restart server
6. Plugin reads .pb file
7. ✅ `UpdateStateRequest::decode()` **succeeds** (protobuf structure is valid)
8. Plugin sends request to server with source="request:agent_A@basic_persistency@startup_restore_nginx-persistent"
9. Server calls `verify_update_request()`
10. ✅ Signature verification **fails** (signature bytes are corrupted)
11. Server logs: "❌ UpdateStateRequest signature verification failed from ..."
12. ✅ Test passes: logs contain "signature verification failed"

## Files Changed

1. **tests/stests/signature_e2e/tamper_signature.py** (NEW)
   - Wire-format protobuf parser
   - Signature corruption logic

2. **tests/stests/signature_e2e/signature_persistence.robot**
   - Lines 264-271: Replace truncation logic with script call

## Verification

To manually test the tamper script:

```bash
# Create a test .pb file first by running Test 3 up to line 263
# Then run:
python3 tests/stests/signature_e2e/tamper_signature.py /tmp/test_workload.pb

# Output should show:
# Original file size: XXX bytes
# ✓ Tampered signature: flipped 8 bytes
# Tampered file size: XXX bytes
# SUCCESS: Tampered /tmp/test_workload.pb
```

## Testing

Run the specific test:
```bash
robot --outputdir test-results \
      --test "Tampered Persistence File Is Rejected On Restore" \
      tests/stests/signature_e2e/signature_persistence.robot
```

Expected result: **PASS** ✅

## Alternative Approaches Considered

### Option 1: Send decode errors to server (Rejected)
**Idea:** Make plugin forward decode errors to server for centralized logging.

**Why rejected:**
- Architectural complexity - plugin would need to send malformed data
- Server can't verify what it can't decode
- Decode errors are legitimate plugin-level failures

### Option 2: Change test expectation (Rejected)
**Idea:** Check for "Failed to decode protobuf" in plugin logs instead.

**Why rejected:**
- Doesn't test signature verification (the actual feature)
- Decode errors are different from signature tampering
- Test should verify security mechanism, not decode failures

### Option 3: Fix tampering method (Selected ✅)
**Idea:** Tamper with signature bytes while preserving protobuf structure.

**Why selected:**
- Tests the actual signature verification mechanism
- Realistic attack scenario (attacker modifies signature field)
- Preserves test intent (verify tamper detection)
- Minimal code changes
