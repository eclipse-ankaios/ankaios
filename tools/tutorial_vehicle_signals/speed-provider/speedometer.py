import os
from kuksa_client.grpc import VSSClient
from kuksa_client.grpc import Datapoint

import time

addr=os.environ.get('KUKSA_DATA_BROKER_ADDR', '127.0.0.1')
port=int(os.environ.get('KUKSA_DATA_BROKER_PORT', '55555'))
interval=float(os.environ.get('SPEEDOMETER_INTERVAL', '1'))

with VSSClient(addr, port) as client:
    while True:
        for speed in range(0,100):
            client.set_current_values({
            'Vehicle.Speed': Datapoint(speed),
            })
            print(f"Feeding Vehicle.Speed to {speed}")
            time.sleep(interval)
        for speed in range(100,0,-1):
            client.set_current_values({
            'Vehicle.Speed': Datapoint(speed),
            })
            print(f"Feeding Vehicle.Speed to {speed}")
            time.sleep(interval)
