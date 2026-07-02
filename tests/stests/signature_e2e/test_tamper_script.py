#!/usr/bin/env python3
"""
Unit test for tamper_signature.py to verify it correctly modifies signature bytes
while preserving protobuf structure.
"""

import sys
import os

# Import the tamper module
sys.path.insert(0, os.path.dirname(__file__))
from tamper_signature import tamper_signature_bytes, parse_varint, encode_varint


def test_varint_encoding():
    """Test varint encoding/decoding roundtrip"""
    test_values = [0, 1, 127, 128, 255, 256, 65535, 65536]

    for value in test_values:
        encoded = encode_varint(value)
        decoded, offset = parse_varint(encoded, 0)
        assert decoded == value, f"Varint roundtrip failed for {value}: got {decoded}"
        assert offset == len(encoded), f"Offset mismatch for {value}"

    print("✓ Varint encoding/decoding tests passed")


def test_protobuf_tampering():
    """Test that tampering preserves protobuf structure"""

    # Create a minimal valid protobuf with signature_metadata
    # Field 3 (signature_metadata) with embedded message containing:
    #   Field 1 (signature) = 64 bytes (Ed25519 signature size)

    signature = b'\x00' * 64  # 64-byte signature

    # Build SignatureMetadata embedded message
    # Field 1 (signature) = tag 0x0A (field 1, wire type 2), length, bytes
    sig_metadata = bytearray()
    sig_metadata.append(0x0A)  # Tag: field 1, wire type 2 (length-delimited)
    sig_metadata.extend(encode_varint(len(signature)))
    sig_metadata.extend(signature)

    # Add Field 2 (key_id) = "test-key"
    key_id = b"test-key"
    sig_metadata.append(0x12)  # Tag: field 2, wire type 2
    sig_metadata.extend(encode_varint(len(key_id)))
    sig_metadata.extend(key_id)

    # Add Field 3 (counter) = 20
    sig_metadata.append(0x18)  # Tag: field 3, wire type 0 (varint)
    sig_metadata.extend(encode_varint(20))

    # Add Field 4 (timestamp) = 1234567890
    sig_metadata.append(0x20)  # Tag: field 4, wire type 0
    sig_metadata.extend(encode_varint(1234567890))

    # Build UpdateStateRequest
    # Field 3 (signature_metadata) = embedded message
    update_request = bytearray()
    update_request.append(0x1A)  # Tag: field 3, wire type 2
    update_request.extend(encode_varint(len(sig_metadata)))
    update_request.extend(sig_metadata)

    original = bytes(update_request)
    print(f"Original protobuf size: {len(original)} bytes")

    # Tamper with signature
    tampered = tamper_signature_bytes(original)
    print(f"Tampered protobuf size: {len(tampered)} bytes")

    # Verify size is preserved (important for protobuf validity)
    assert len(tampered) == len(original), "Protobuf size changed after tampering"

    # Verify first 8 bytes of signature were flipped
    # Parse to find the signature field
    offset = 0
    tag, offset = parse_varint(tampered, offset)
    assert tag == 0x1A, "Field 3 tag missing"

    length, offset = parse_varint(tampered, offset)
    embedded_start = offset

    # Parse embedded message
    emb_tag, offset = parse_varint(tampered, embedded_start)
    assert emb_tag == 0x0A, "Signature field tag missing"

    sig_length, offset = parse_varint(tampered, offset)
    assert sig_length == 64, f"Signature length changed: {sig_length}"

    tampered_sig = tampered[offset:offset + 64]

    # Verify first 8 bytes are flipped
    for i in range(8):
        expected = signature[i] ^ 0xFF
        actual = tampered_sig[i]
        assert actual == expected, f"Byte {i} not flipped correctly: {actual:02x} != {expected:02x}"

    # Verify remaining bytes are unchanged
    for i in range(8, 64):
        assert tampered_sig[i] == signature[i], f"Byte {i} should not be modified"

    print("✓ Protobuf tampering test passed")
    print("  - Structure preserved (same size)")
    print("  - First 8 signature bytes flipped")
    print("  - Remaining signature bytes unchanged")


if __name__ == "__main__":
    try:
        test_varint_encoding()
        test_protobuf_tampering()
        print("\n✅ All tests passed!")
        sys.exit(0)
    except AssertionError as e:
        print(f"\n❌ Test failed: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
