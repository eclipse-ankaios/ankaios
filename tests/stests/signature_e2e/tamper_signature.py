#!/usr/bin/env python3
"""
Helper script to tamper with signature in a protobuf-encoded UpdateStateRequest.

This ensures the protobuf structure remains valid (so it can be decoded) but the
signature verification fails, which is what we want to test.

This uses low-level protobuf wire format manipulation to avoid needing generated code.
"""

import sys
import os


def parse_varint(data: bytes, offset: int) -> tuple[int, int]:
    """Parse a protobuf varint, return (value, new_offset)"""
    value = 0
    shift = 0
    while True:
        if offset >= len(data):
            raise ValueError("Unexpected end of varint")
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7F) << shift
        if not (byte & 0x80):
            break
        shift += 7
    return value, offset


def encode_varint(value: int) -> bytes:
    """Encode an integer as a protobuf varint"""
    result = bytearray()
    while value > 0x7F:
        result.append((value & 0x7F) | 0x80)
        value >>= 7
    result.append(value & 0x7F)
    return bytes(result)


def tamper_signature_bytes(pb_bytes: bytes) -> bytes:
    """
    Parse protobuf wire format, find signature field, and corrupt it.

    UpdateStateRequest wire format:
      Field 3 (signature_metadata) = embedded message
        Field 1 (signature) = bytes
        Field 2 (key_id) = string
        Field 3 (counter) = uint64
        Field 4 (timestamp) = uint64

    Wire type encoding:
      - 0: varint
      - 2: length-delimited (strings, bytes, embedded messages)
    """
    result = bytearray()
    offset = 0
    tampered = False

    while offset < len(pb_bytes):
        # Read field tag
        if offset >= len(pb_bytes):
            break
        tag, offset = parse_varint(pb_bytes, offset)
        field_number = tag >> 3
        wire_type = tag & 0x07

        # Field 3 is signature_metadata (embedded message)
        if field_number == 3 and wire_type == 2:
            # Read length of embedded message
            length, new_offset = parse_varint(pb_bytes, offset)
            embedded_start = new_offset
            embedded_end = new_offset + length

            # Write the tag and length as-is
            result.append(tag)
            result.extend(encode_varint(length))

            # Parse the embedded message (SignatureMetadata)
            embedded_offset = embedded_start
            embedded_result = bytearray()

            while embedded_offset < embedded_end:
                emb_tag, embedded_offset = parse_varint(pb_bytes, embedded_offset)
                emb_field_number = emb_tag >> 3
                emb_wire_type = emb_tag & 0x07

                # Field 1 is signature (bytes)
                if emb_field_number == 1 and emb_wire_type == 2:
                    sig_length, sig_offset = parse_varint(pb_bytes, embedded_offset)
                    signature = bytearray(pb_bytes[sig_offset:sig_offset + sig_length])

                    # Tamper: flip first 8 bytes
                    for i in range(min(8, len(signature))):
                        signature[i] ^= 0xFF

                    # Write tampered signature
                    embedded_result.append(emb_tag)
                    embedded_result.extend(encode_varint(len(signature)))
                    embedded_result.extend(signature)

                    embedded_offset = sig_offset + sig_length
                    tampered = True
                    print(f"✓ Tampered signature: flipped {min(8, len(signature))} bytes", file=sys.stderr)

                else:
                    # Copy other fields as-is
                    embedded_result.append(emb_tag)
                    if emb_wire_type == 0:
                        # Varint
                        value, embedded_offset = parse_varint(pb_bytes, embedded_offset)
                        embedded_result.extend(encode_varint(value))
                    elif emb_wire_type == 2:
                        # Length-delimited
                        length2, new_off = parse_varint(pb_bytes, embedded_offset)
                        embedded_result.extend(encode_varint(length2))
                        embedded_result.extend(pb_bytes[new_off:new_off + length2])
                        embedded_offset = new_off + length2

            result.extend(embedded_result)
            offset = embedded_end

        else:
            # Copy other fields as-is
            result.append(tag)
            if wire_type == 0:
                # Varint
                value, offset = parse_varint(pb_bytes, offset)
                result.extend(encode_varint(value))
            elif wire_type == 2:
                # Length-delimited (string, bytes, embedded message)
                length, new_offset = parse_varint(pb_bytes, offset)
                result.extend(encode_varint(length))
                result.extend(pb_bytes[new_offset:new_offset + length])
                offset = new_offset + length

    if not tampered:
        print("ERROR: No signature field found to tamper", file=sys.stderr)
        sys.exit(1)

    return bytes(result)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <path-to-pb-file>", file=sys.stderr)
        sys.exit(1)

    pb_file = sys.argv[1]

    if not os.path.exists(pb_file):
        print(f"ERROR: File not found: {pb_file}", file=sys.stderr)
        sys.exit(1)

    # Read original file
    with open(pb_file, "rb") as f:
        original = f.read()

    print(f"Original file size: {len(original)} bytes", file=sys.stderr)

    # Tamper with signature
    try:
        tampered = tamper_signature_bytes(original)
    except Exception as e:
        print(f"ERROR: Failed to tamper signature: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc(file=sys.stderr)
        sys.exit(1)

    print(f"Tampered file size: {len(tampered)} bytes", file=sys.stderr)

    # Write back
    with open(pb_file, "wb") as f:
        f.write(tampered)

    print(f"SUCCESS: Tampered {pb_file}")
