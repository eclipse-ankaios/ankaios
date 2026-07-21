#!/bin/bash
# Verification script for signature E2E testing infrastructure
# This script verifies that all components of the testing infrastructure are working

set -e

echo "================================================"
echo "Signature E2E Testing Infrastructure Verification"
echo "================================================"
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track overall status
FAILURES=0

# Function to check status
check_status() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ PASS${NC}: $1"
    else
        echo -e "${RED}❌ FAIL${NC}: $1"
        FAILURES=$((FAILURES + 1))
    fi
}

echo "Step 1: Checking Python dependencies..."
python3 -c "import cryptography, yaml" 2>/dev/null
check_status "Python cryptography module"

if python3 -c "import robot" 2>/dev/null; then
    check_status "Robot Framework"
else
    echo -e "${YELLOW}⚠ WARN${NC}: Robot Framework not installed (optional for local testing, required for CI/CD)"
    echo "  Install: pip3 install robotframework"
fi

echo ""
echo "Step 2: Checking helper scripts exist..."
test -f tests/resources/generate_keypair.py
check_status "generate_keypair.py exists"

test -f tests/resources/sign_manifest.py
check_status "sign_manifest.py exists"

test -f tests/resources/verify_signature.py
check_status "verify_signature.py exists"

test -f tests/resources/signature_utils.resource
check_status "signature_utils.resource exists"

echo ""
echo "Step 3: Building ank CLI..."
cargo build --release --bin ank --target x86_64-unknown-linux-gnu >/dev/null 2>&1
check_status "ank CLI build"

echo ""
echo "Step 4: Keypair generation..."
TEMP_KEYS=$(mktemp -d)
./target/x86_64-unknown-linux-gnu/release/ank keygen --output "$TEMP_KEYS/verify-test.pem" >/dev/null 2>&1
check_status "Keypair generation (ank keygen)"
test -f "$TEMP_KEYS/verify-test.pem"
check_status "Private key created"
test -f "$TEMP_KEYS/verify-test.pem.pub"
check_status "Public key created"

echo ""
echo "Step 5: Manifest signing..."
TEMP_MANIFEST=$(mktemp --suffix=.yaml)
cat > "$TEMP_MANIFEST" << 'EOF'
apiVersion: v1
workloads:
  test:
    runtime: podman
    agent: agent_A
    runtimeConfig: |
      image: nginx:latest
EOF
./target/x86_64-unknown-linux-gnu/release/ank sign --input "$TEMP_MANIFEST" --key "$TEMP_KEYS/verify-test.pem" --key-id verify-test --counter 42 >/dev/null 2>&1
check_status "Manifest signing (ank sign)"

echo ""
echo "Step 6: Signature verification..."
echo -e "${YELLOW}⏸ SKIP${NC}: ank verify command not yet implemented"

echo ""
echo "Step 7: Checking test fixtures..."
test -f tests/stests/signature_e2e/fixtures/signed_workload.yaml
check_status "signed_workload.yaml exists"

test -f tests/stests/signature_e2e/fixtures/unsigned_workload.yaml
check_status "unsigned_workload.yaml exists"

echo ""
echo "Step 8: Checking Robot Framework test suite..."
test -f tests/stests/signature_e2e/signature_persistence.robot
check_status "signature_persistence.robot exists"

# Count test cases in robot file (look for lines with [Documentation] that are indented - those are test cases)
TEST_COUNT=$(grep "^    \[Documentation\]" tests/stests/signature_e2e/signature_persistence.robot | wc -l)
if [ "$TEST_COUNT" -ge 5 ]; then
    check_status "Test suite has $TEST_COUNT test cases (expected >= 5)"
else
    echo -e "${RED}❌ FAIL${NC}: Test suite has only $TEST_COUNT test cases (expected >= 5)"
    FAILURES=$((FAILURES + 1))
fi

echo ""
echo "Step 9: Checking documentation..."
test -f tests/stests/signature_e2e/README.md
check_status "README.md exists"

test -f tests/stests/signature_e2e/TESTING_GUIDE.md
check_status "TESTING_GUIDE.md exists"

test -f tests/stests/signature_e2e/IMPLEMENTATION_SUMMARY.md
check_status "IMPLEMENTATION_SUMMARY.md exists"

echo ""
echo "Step 10: Checking Rust integration tests..."
test -f server/tests/signature_flow_integration.rs
check_status "signature_flow_integration.rs exists"

echo ""
echo "Step 11: Running Rust integration tests..."
cargo test --package ank-server --test signature_flow_integration --target x86_64-unknown-linux-gnu -- --nocapture >/dev/null 2>&1
check_status "Rust integration tests pass"

echo ""
echo "Step 12: Checking CI/CD workflow..."
test -f .github/workflows/signature-e2e-tests.yml
check_status "GitHub Actions workflow exists"

echo ""
echo "Step 13: Cleanup test files..."
rm -rf "$TEMP_KEYS" "$TEMP_MANIFEST"
check_status "Cleanup"

echo ""
echo "================================================"
echo "Verification Summary"
echo "================================================"

if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}✅ ALL CHECKS PASSED${NC}"
    echo ""
    echo "Testing infrastructure is fully operational!"
    echo ""
    echo "Next steps:"
    echo "1. Run Robot Framework tests:"
    echo "   robot --outputdir test-results tests/stests/signature_e2e/"
    echo ""
    echo "2. Run Rust integration tests:"
    echo "   cargo test --package ank-server --test signature_flow_integration"
    echo ""
    echo "3. Check test results in test-results/ directory"
    exit 0
else
    echo -e "${RED}❌ $FAILURES CHECK(S) FAILED${NC}"
    echo ""
    echo "Please fix the failures above before running tests."
    exit 1
fi
