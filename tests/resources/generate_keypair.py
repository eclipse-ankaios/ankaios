#!/usr/bin/env python3
"""Generate Ed25519 keypair for testing"""

import sys
import os
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization

def generate_keypair(key_id: str, output_dir: str):
    os.makedirs(output_dir, exist_ok=True)

    # Generate private key
    private_key = Ed25519PrivateKey.generate()

    # Save private key (PEM format)
    private_pem = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    )

    private_path = os.path.join(output_dir, f"{key_id}.pem")
    with open(private_path, 'wb') as f:
        f.write(private_pem)
    os.chmod(private_path, 0o600)

    # Save public key (PEM format)
    public_key = private_key.public_key()
    public_pem = public_key.public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo
    )

    public_path = os.path.join(output_dir, f"{key_id}.pub")
    with open(public_path, 'wb') as f:
        f.write(public_pem)

    print(f"Generated keypair: {key_id}")
    return 0

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print("Usage: generate_keypair.py <key_id> <output_dir>")
        sys.exit(1)
    sys.exit(generate_keypair(sys.argv[1], sys.argv[2]))
