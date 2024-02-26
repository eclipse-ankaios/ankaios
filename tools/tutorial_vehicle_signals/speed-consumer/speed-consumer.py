import os
import sys
from datetime import datetime
from kuksa_client.grpc import VSSClient
from kuksa_client.grpc import Datapoint

def log(msg):
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"{now} {msg}", file=sys.stderr, flush=True)

addr=os.environ.get('KUKSA_DATA_BROKER_ADDR', '127.0.0.1')
port=int(os.environ.get('KUKSA_DATA_BROKER_PORT', '55555'))

with VSSClient(addr, port) as client:

    for updates in client.subscribe_current_values([
        'Vehicle.Speed',
    ]):
        speed = updates['Vehicle.Speed']
        if speed is not None:
            log(f"Received updated speed: {speed.value}")
