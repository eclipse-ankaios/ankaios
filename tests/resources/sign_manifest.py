#!/usr/bin/env python3
"""Sign YAML manifest with Ed25519 signature"""

import sys
import time
import base64
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

def sign_manifest(manifest_path: str, private_key_path: str, counter: int):
    # Read manifest
    with open(manifest_path, 'r') as f:
        yaml_content = f.read()

    # Strip existing signature if present
    if '\n---\n' in yaml_content:
        unsigned_content = yaml_content.split('\n---\n')[0]
    else:
        unsigned_content = yaml_content

    # Ensure unsigned content ends with newline (YAML files should end with newline)
    if not unsigned_content.endswith('\n'):
        unsigned_content = unsigned_content + '\n'

    # Load private key
    with open(private_key_path, 'rb') as f:
        private_key = serialization.load_pem_private_key(f.read(), password=None)

    # Sign
    signature_bytes = private_key.sign(unsigned_content.encode('utf-8'))
    signature_b64 = base64.b64encode(signature_bytes).decode('ascii')

    # Get key_id from filename
    import os
    key_id = os.path.basename(private_key_path).replace('.pem', '')

    # Build signature block
    timestamp = int(time.time())
    signature_block = f"""---
# Ankaios Signature Block v1
signature: {signature_b64}
key_id: {key_id}
timestamp: {timestamp}
counter: {counter}
"""

    # Write signed manifest
    signed_yaml = unsigned_content + signature_block
    with open(manifest_path, 'w') as f:
        f.write(signed_yaml)

    print(f"Signed manifest: {manifest_path} (counter={counter})")
    return 0

if __name__ == '__main__':
    if len(sys.argv) != 4:
        print("Usage: sign_manifest.py <manifest_path> <private_key_path> <counter>")
        sys.exit(1)
    sys.exit(sign_manifest(sys.argv[1], sys.argv[2], int(sys.argv[3])))
