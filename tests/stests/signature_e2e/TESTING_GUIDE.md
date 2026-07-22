# Signature E2E Testing Guide

## Quick Start

### Prerequisites Check
```bash
# Check Python dependencies
python3 -c "import cryptography, yaml, robot" && echo "✅ All Python dependencies installed" || echo "❌ Missing dependencies"

# Install if needed
pip3 install robotframework cryptography pyyaml

# Check Ankaios binaries
which ank-server ank-persist ank || echo "Build binaries: cargo build --release"
```

### Run All Tests
```bash
cd /home/pierrey/repos/gitrepo/ankaios

# Robot Framework E2E tests
robot --outputdir test-results tests/stests/signature_e2e/

# Rust integration tests
cargo test --package ank-server --test signature_flow_integration
```

### Run Specific Test
```bash
# Just the critical signature preservation test
robot --test "Signed Manifest Is Persisted With Signature Block" \
     --outputdir test-results \
     tests/stests/signature_e2e/signature_persistence.robot
```

## Test Results Interpretation

### ✅ Success Output
```
Signed Manifest Is Persisted With Signature Block    | PASS |
✅ SUCCESS: Persistence file contains valid signature
==============================================================================
Signature Persistence :: End-to-end tests for signature... | PASS |
5 tests, 5 passed, 0 failed
```

### ❌ Failure Output  
If you see this failure:
```
Persistence file must contain signature field
```

**Cause:** The signed_yaml storage bug is still present - server not preserving signed_yaml through GetStateRequest.

**Fix verify:**
```bash
# Check if server_state.rs has get_last_signed_yaml()
grep -n "get_last_signed_yaml" server/src/ankaios_server/server_state.rs

# Check if ankaios_server.rs calls it
grep -n "get_last_signed_yaml" server/src/ankaios_server.rs
```

## Manual Testing

### Quick Smoke Test
```bash
# 1. Generate test keypair
python3 tests/resources/generate_keypair.py test-key /tmp/test-keys

# 2. Create and sign manifest
cat > /tmp/test.yaml << 'EOF'
apiVersion: v1
workloads:
  nginx:
    runtime: podman
    agent: agent_A
    runtimeConfig: |
      image: nginx:latest
EOF

python3 tests/resources/sign_manifest.py /tmp/test.yaml /tmp/test-keys/test-key.pem 1

# 3. Verify signature was added
tail -5 /tmp/test.yaml
# Should show:
# ---
# # Ankaios Signature Block v1
# signature: <base64>
# key_id: test-key
# counter: 1

# 4. Verify signature is valid
python3 tests/resources/verify_signature.py /tmp/test.yaml /tmp/test-keys/test-key.pub
# Should print: ✅ Signature valid (key_id=test-key, counter=1)
```

## Test Coverage Matrix

| Test Scenario | Robot E2E | Rust Unit | Coverage Area |
|---------------|-----------|-----------|---------------|
| Ed25519 signing/verification | ✅ | ✅ | Cryptographic primitives |
| Signature block format | ✅ | ✅ | YAML structure |
| UpdateStateRequest → GetStateRequest flow | ✅ | ❌* | Server integration |
| Persistence file signature preservation | ✅ | ❌* | Plugin integration |
| Server restart state restoration | ✅ | ❌* | Full lifecycle |
| Tampering detection | ✅ | ✅ | Security |
| Counter rollback prevention | ✅ | ❌* | Security |
| Policy enforcement (require_signature) | ✅ | ❌* | Configuration |

\* Rust unit tests cover the helpers but full server integration is tested via Robot Framework

## Debugging Failed Tests

### Check Server Logs
```bash
tail -100 /tmp/ankaios-server.log
```

**Look for:**
- `✅ Signature verified` - signature validation succeeded
- `❌ Signature verification failed` - signature invalid or tampered
- `Counter rollback detected` - replay attack prevented
- `Signature required by policy` - unsigned manifest rejected

### Check Persistence Plugin Logs
```bash
tail -100 /tmp/ank-persist.log
```

**Look for:**
- `Persistence file requires signed YAML` - plugin enforcing signature requirement
- Errors about missing signature blocks

### Inspect Persisted State
```bash
# View full persisted file
cat /tmp/runtime_state_*.yaml

# Check for signature block
grep -A5 "^---" /tmp/runtime_state_*.yaml

# Verify signature is valid
python3 tests/resources/verify_signature.py \
    /tmp/runtime_state_*.yaml \
    /tmp/test-keys/*/test-key-*.pub
```

### Test Helpers Directly
```bash
# Generate fresh keypair
python3 tests/resources/generate_keypair.py debug-key /tmp/debug

# Sign test manifest
echo "apiVersion: v1" > /tmp/debug.yaml
echo "workloads: {}" >> /tmp/debug.yaml
python3 tests/resources/sign_manifest.py /tmp/debug.yaml /tmp/debug/debug-key.pem 99

# Verify
python3 tests/resources/verify_signature.py /tmp/debug.yaml /tmp/debug/debug-key.pub
```

## CI/CD Integration

Tests run automatically on every commit via GitHub Actions:

**.github/workflows/signature-e2e-tests.yml**

**Workflow steps:**
1. Install Python dependencies (robotframework, cryptography, pyyaml)
2. Build Ankaios binaries (ank-server, ank-persist, ank)
3. Run Robot Framework E2E tests
4. Run Rust integration tests
5. Upload test results as artifacts
6. Display test summary in workflow output

**View results:**
- GitHub Actions → Workflow runs → "Signature E2E Tests"
- Download "robot-test-results" artifact for detailed logs

## Common Issues

### Issue: Python module not found
```
ModuleNotFoundError: No module named 'cryptography'
```

**Fix:**
```bash
pip3 install cryptography pyyaml robotframework
```

### Issue: Test timeout waiting for workload
```
Timeout waiting for workload to reach Running state
```

**Fix:** Tests spawn real server but may not have agent running. This is expected - tests verify signature preservation, not actual workload execution.

### Issue: Permission denied on /tmp files
```
PermissionError: [Errno 13] Permission denied: '/tmp/test-keys/test-key.pem'
```

**Fix:**
```bash
rm -rf /tmp/test-keys* /tmp/runtime_state*.yaml /tmp/ank*.log
```

### Issue: Signature verification failed (fresh test)
**Cause:** Trailing newline mismatch between signing and verification.

**Already fixed in:** sign_manifest.py and verify_signature.py normalize trailing newlines.

## Test Maintenance

### Adding New Test Scenarios

1. **Add test case to signature_persistence.robot:**
```robot
*** Test Cases ***
My New Security Test
    [Documentation]    What this test verifies and why it matters
    [Tags]    signature    security    critical
    
    Generate Ed25519 Keypair    test-key-new    ${KEYS_DIR}
    # ... test steps using keywords from signature_utils.resource
    
    Log    ✅ SUCCESS: What was verified
    
    [Teardown]    Cleanup steps
```

2. **Add test fixture if needed:**
   - Create YAML file in `fixtures/` directory
   - Template will be signed during test execution

3. **Add Rust unit test if testing helpers:**
   - Edit `server/tests/signature_flow_integration.rs`
   - Add test function in `signature_flow_tests` module

### Updating Test Infrastructure

**Python helpers:** `/tests/resources/*.py`
- `generate_keypair.py` - Keypair generation
- `sign_manifest.py` - YAML signing  
- `verify_signature.py` - Signature verification

**Robot keywords:** `/tests/resources/signature_utils.resource`
- `Generate Ed25519 Keypair`
- `Sign Manifest`
- `Verify Manifest Signature`
- `Start Ankaios Server` / `Stop Ankaios Server`
- `Apply Manifest`
- `Get Workloads`

## Security Test Validation

### Verify Security Properties

**Property 1: Signature preservation**
```bash
# Apply signed manifest
robot --test "Signed Manifest Is Persisted" tests/stests/signature_e2e/

# Verify persistence file has signature
grep "signature:" /tmp/runtime_state*.yaml
```

**Property 2: Tampering detection**
```bash
# Run tampering test
robot --test "Tampered Persistence File" tests/stests/signature_e2e/

# Logs should show: "Signature verification failed"
grep "Signature verification failed" /tmp/ankaios-server.log
```

**Property 3: Replay prevention**
```bash
# Run counter rollback test
robot --test "Counter Rollback Attack" tests/stests/signature_e2e/

# Logs should show: "Counter rollback detected"
grep "Counter rollback" /tmp/ankaios-server.log
```

## Performance

**Test execution time:**
- Rust unit tests: ~0.02s (5 tests)
- Robot Framework E2E: ~30-60s (5 tests)
  - Includes server startup, workload application, restart cycles

**CI/CD duration:** ~3-5 minutes total
- Build: ~2-3 minutes
- Tests: ~1-2 minutes

## Support

**Issues with tests?**
- Check test logs: `test-results/log.html` (Robot Framework)
- Check server logs: `/tmp/ankaios-server.log`
- Check plugin logs: `/tmp/ank-persist.log`

**Questions about test coverage?**
- See `TEST_COVERAGE_ANALYSIS.md` in plan documentation
- See test documentation in `signature_flow_integration.rs`

**Found a bug?**
- These tests exist because the signed_yaml storage bug was missed
- If tests fail, investigate - they caught a critical integration bug before
