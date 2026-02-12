import os
import time
import sys
from datetime import datetime
from kuksa_client.grpc import VSSClient
from kuksa_client.grpc import Datapoint
from flask import Flask, request, render_template

addr=os.environ.get('KUKSA_DATA_BROKER_ADDR', '127.0.0.1')
port=int(os.environ.get('KUKSA_DATA_BROKER_PORT', '55555'))
mode=os.environ.get('SPEED_PROVIDER_MODE', 'webui')

app = Flask(__name__, template_folder='templates')

def log(msg):
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"{now} {msg}", file=sys.stderr, flush=True)

@app.route('/', methods=['GET', 'POST'])
def index():
    if request.method == 'POST':
        if 'vehiclespeed' in request.form:
            speed = float(request.form['vehiclespeed'])
            with VSSClient(addr, port) as client:
                client.set_current_values({'Vehicle.Speed': Datapoint(speed),})
                log(f"Feeding Vehicle.Speed to {speed}")
                result = "Vehicle.Speed of {} km/h has been sent.".format(speed)
                return render_template('index.html', result=result)
    return render_template('index.html')

def automatic():
    with VSSClient(addr, port) as client:
        while True:
            for speed in range(0,100):
                client.set_current_values({
                'Vehicle.Speed': Datapoint(speed),
                })
                log(f"Feeding Vehicle.Speed to {speed}")
                time.sleep(1)
            for speed in range(100,0,-1):
                client.set_current_values({
                'Vehicle.Speed': Datapoint(speed),
                })
                log(f"Feeding Vehicle.Speed to {speed}")
                time.sleep(1)

if __name__ == '__main__':
    if mode == 'webui':
        log("Web UI mode")
        app.run(host='0.0.0.0', port=5000)
    elif mode == 'auto':
        log("Automatic mode")
        automatic()
