#!/usr/bin/env python3
"""Verify Ed25519 signature on manifest"""

import sys
import base64
import yaml
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.exceptions import InvalidSignature

def verify_signature(manifest_path: str, public_key_path: str):
    # Read manifest
    with open(manifest_path, 'r') as f:
        yaml_content = f.read()

    # Split signature block
    parts = yaml_content.split('\n---\n')
    if len(parts) < 2:
        print("ERROR: No signature block found")
        return 1

    unsigned_content = parts[0]
    signature_yaml = parts[1]

    # Ensure unsigned content ends with newline (YAML files should end with newline)
    if not unsigned_content.endswith('\n'):
        unsigned_content = unsigned_content + '\n'

    # Parse signature block
    sig_block = yaml.safe_load(signature_yaml)
    signature_bytes = base64.b64decode(sig_block['signature'])

    # Load public key
    with open(public_key_path, 'rb') as f:
        public_key = serialization.load_pem_public_key(f.read())

    # Verify
    try:
        public_key.verify(signature_bytes, unsigned_content.encode('utf-8'))
        print(f"✅ Signature valid (key_id={sig_block['key_id']}, counter={sig_block['counter']})")
        return 0
    except InvalidSignature:
        print("❌ Signature verification failed")
        return 1

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print("Usage: verify_signature.py <manifest_path> <public_key_path>")
        sys.exit(1)
    sys.exit(verify_signature(sys.argv[1], sys.argv[2]))
