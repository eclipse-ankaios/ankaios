import os
from kuksa_client.grpc import VSSClient
from kuksa_client.grpc import Datapoint

import time

addr=os.environ.get('KUKSA_DATA_BROKER_ADDR', '127.0.0.1')
port=int(os.environ.get('KUKSA_DATA_BROKER_PORT', '55555'))

from kuksa_client.grpc import VSSClient

with VSSClient(addr, port) as client:

    for updates in client.subscribe_current_values([
        'Vehicle.Speed',
    ]):
        speed = updates['Vehicle.Speed'].value
        print(f"Received updated speed: {speed}")
