from ankaios_sdk import Workload, Ankaios, WorkloadStateEnum, WorkloadSubStateEnum, AnkaiosLogLevel, Manifest, Request, CompleteState
import paho.mqtt.client as mqtt
import json
import os
import logging
import sys
import signal

logger = logging.getLogger("fleetconnector")
stdout = logging.StreamHandler(stream=sys.stdout)
stdout.setLevel(logging.INFO)
logger.addHandler(stdout)
logger.setLevel(logging.INFO)

# Configuration for MQTT  broker and topics
BROKER = os.environ.get('MQTT_BROKER_ADDR', 'localhost')
PORT = int(os.environ.get('MQTT_BROKER_PORT', '1883'))
VEHICLE_ID = os.environ.get('VIN')
BASE_TOPIC = f"vehicle/{VEHICLE_ID}"

# Create a new Ankaios object.
# The connection to the control interface is automatically done at this step.
# The Ankaios class supports context manager syntax:
with Ankaios() as ankaios:

    # Callback when the client receives a CONNACK response from the MQTT server
    def on_connect(client, userdata, flags, reason_code, properties):
        client.subscribe(f"{BASE_TOPIC}/manifest/apply/req")
        client.subscribe(f"{BASE_TOPIC}/manifest/delete/req")
        client.subscribe(f"{BASE_TOPIC}/state/req")

    # Callback when a PUBLISH message is received from the MQTT server
    def on_message(client, userdata, msg):
        try:
            logger.info(f"Received message on topic {msg.topic} with payload {msg.payload.decode()}")
            # Handle request for applying a manifest
            if msg.topic == f"{BASE_TOPIC}/manifest/apply/req":
                manifest = Manifest.from_string(str(msg.payload.decode()))
                ret = ankaios.apply_manifest(manifest)
                if ret is not None:
                    client.publish(f"{BASE_TOPIC}/manifest/apply/resp", json.dumps(ret.to_dict()))
            # Handle request for deleting a manifest
            elif msg.topic == f"{BASE_TOPIC}/manifest/delete/req":
                manifest = Manifest.from_string(str(msg.payload.decode()))
                ret = ankaios.delete_manifest(manifest)
                if ret is not None:
                    client.publish(f"{BASE_TOPIC}/manifest/delete/resp", json.dumps(ret.to_dict()))
            # Handle request for getting the state of Ankaios
            elif msg.topic == f"{BASE_TOPIC}/state/req":
                state = ankaios.get_state(field_masks=json.loads(str(msg.payload.decode())))
                client.publish(f"{BASE_TOPIC}/state/resp", json.dumps(state.to_dict()))
        except Exception as e:
            logger.error(f"Error processing message: {e}")

    def signal_handler(sig, frame):
        ankaios.disconnect()
        sys.exit(0)

    # Create an MQTT client instance
    mqtt_client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)

    # Assign the callbacks
    mqtt_client.on_connect = on_connect
    mqtt_client.on_message = on_message

    # Connect to the MQTT broker
    mqtt_client.connect(BROKER, PORT, 60)

    signal.signal(signal.SIGTERM, signal_handler)

    # Blocking call that processes network traffic, dispatches callbacks,
    # and handles reconnecting.
    mqtt_client.loop_forever()
