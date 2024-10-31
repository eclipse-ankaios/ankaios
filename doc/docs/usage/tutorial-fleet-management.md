# Tutorial: Manage a fleet of vehicles from the cloud

## Introduction

This tutorial will show you how to manage a fleet of vehicles that are running Ankaios. We will remotely start new workloads on the vehicle and update existing one.
The reader should be familiar with Ankaios' basics as provided in the tutorial [Sending and receiving vehicle signals](tutorial-vehicle-signals.md).

For connecting the vehicles with the cloud we use an MQTT connection. Every vehicle connects to a central MQTT broker. The connection from the vehicle is established by a fleet connector workload which is managed by Ankaios. Using an Ankaios workload has the advantage, that workloads get direct access to the Ankaios control interface which enables them to start, stop and update other workloads.

<figure markdown>
  ![Overview of workloads](../assets/tutorial_fleet_management.png)
  <figcaption>Fleet management overview</figcaption>
</figure>

To run this tutorial you will need a Linux platform, which can be a WSL2, RaspberryPi, a Linux PC or virtual machine.
Additionally, it's assumed that the Ankaios setup is done with mutual TLS (mTLS) disabled or using its default installation settings.

## MQTT broker

In real world, the MQTT broker would reside in the cloud.
But for this tutorial we setup an MQTT broker on the local machine.

```shell
podman run -d -p 1883:1883 docker.io/eclipse-mosquitto
```

This will start a broker on localhost listening to port 1883. For production use cases MQTT would use TLS and access control which is skipped here for simplicity.

We separate the messages for the different vehicles by using the following topic scheme:

```text
vehicle/<VIN>/
```

Our example vehicle gets the VIN 1. Let's listen to all the messages from and to that vehicle.

```shell
podman run --net=host docker.io/eclipse-mosquitto mosquitto_sub -h localhost -t "vehicle/1/#" -v
```

Keep this window open throughout this tutorial.

## Fleet connector

The fleet connector is a containerized workload managed by Ankaios. It will have two connections:

1. MQTT connection to the cloud in order to receive messages for starting, stopping and updating workloads in the vehicle and to return the response.
2. Connection to the Ankaios [control interface](../reference/control-interface.md) in order to execute the instructions for starting, stopping and updating workloads.

The control interface is provided to every workload via named pipes (FIFO) using a protobuf IDL. In this tutorial we use the [ank-sdk-python](https://github.com/eclipse-ankaios/ank-sdk-python) which provides a convenient way to access the control interface.

Let's have a look at the fleet connector implementation:

```python
from ankaios_sdk import Workload, Ankaios, WorkloadStateEnum, WorkloadSubStateEnum, AnkaiosLogLevel, Manifest, Request, CompleteState
import paho.mqtt.client as mqtt
import json
import os
import logging

logger = logging.getLogger(__name__)
logging.basicConfig(format='%(asctime)s %(message)s', datefmt="[%F %T]", level=logging.INFO)

# Configuration for MQTT  broker and topics
BROKER = os.environ.get('MQTT_BROKER_ADDR', 'localhost')
PORT = int(os.environ.get('MQTT_BROKER_PORT', '1883'))
VEHICLE_ID = os.environ.get('VIN')
BASE_TOPIC = f"vehicle/{VEHICLE_ID}"

# Create a new Ankaios object.
# The connection to the control interface is automatically done at this step.
ankaios = Ankaios()

# Callback when the client receives a CONNACK response from the MQTT server
def on_connect(client, userdata, flags, reason_code, properties):
    client.subscribe(f"{BASE_TOPIC}/manifest/apply/req")
    client.subscribe(f"{BASE_TOPIC}/manifest/delete/req")
    client.subscribe(f"{BASE_TOPIC}/state/req")

def convert_manifest_result_to_dict(result):
    dict = { "added_workloads": [], "deleted_workloads": [] }
    for a in result["added_workloads"]:
        dict["added_workloads"].append(a.__dict__)
    for d in result["deleted_workloads"]:
        dict["deleted_workloads"].append(d.__dict__)
    return dict

# Callback when a PUBLISH message is received from the MQTT server
def on_message(client, userdata, msg):
    try:
        logger.info(f"Received message on topic {msg.topic} with payload {msg.payload.decode()}")
        # Handle request for applying a manifest
        if msg.topic == f"{BASE_TOPIC}/manifest/apply/req":
            manifest = Manifest.from_string(str(msg.payload.decode()))
            ret = ankaios.apply_manifest(manifest)
            if ret is not None:
                client.publish(f"{BASE_TOPIC}/manifest/apply/resp", json.dumps(convert_manifest_result_to_dict(ret)))
        # Handle request for deleting a manifest
        if msg.topic == f"{BASE_TOPIC}/manifest/delete/req":
            manifest = Manifest.from_string(str(msg.payload.decode()))
            ret = ankaios.delete_manifest(manifest)
            if ret is not None:
                client.publish(f"{BASE_TOPIC}/manifest/delete/resp", json.dumps(convert_manifest_result_to_dict(ret)))
        # Handle request for getting the state of Ankaios
        elif msg.topic == f"{BASE_TOPIC}/state/req":
            state = ankaios.get_state(field_masks=json.loads(str(msg.payload.decode())))
            client.publish(f"{BASE_TOPIC}/state/resp", json.dumps(state.to_dict()))
    except Exception as e:
        logger.error(f"Error processing message: {e}")

# Create an MQTT client instance
mqtt_client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)

# Assign the callbacks
mqtt_client.on_connect = on_connect
mqtt_client.on_message = on_message

# Connect to the MQTT broker
mqtt_client.connect(BROKER, PORT, 60)

# Blocking call that processes network traffic, dispatches callbacks,
# and handles reconnecting.
mqtt_client.loop_forever()
```

This python script will run inside the container. With

```python
ankaios = Ankaios()
```

a connection to the Ankaios control interface will be established. After connecting to the MQTT broker with

```python
mqtt_client.connect(BROKER, PORT, 60)
```

the script will listen to incoming MQTT messages.

* **`vehicle/<VIN>/manifest/apply/req`**: Using this topic a remote operator can send an Ankaios manifest which gets applied by the fleet connector using

    ```python
    ret = ankaios.apply_manifest(manifest)
    ```

* **`vehicle/<VIN>/manifest/delete/req`**: Using this topic a remote operator can send an Ankaios manifest which gets deleted by the fleet connector using

    ```python
    ret = ankaios.delete_manifest(manifest)
    ```

* **`vehicle/<VIN>/state/req`**: Using this topic a remote operator can request the current Ankaios state from the fleet connector using

    ```python
     state = ankaios.get_state(field_masks=json.loads(str(msg.payload.decode())))
    ```

    The field mask needs to be provided in JSON format in the message. We will see an example later on in this tutorial.

The complete source code for the fleet connector is available in the [Anakios repository](https://github.com/eclipse-ankaios/ankaios/tree/main/tools/tutorial_fleet_management/fleet-connector). The ank-sdk-python provides many more functions as shown in the [documentation](https://eclipse-ankaios.github.io/ank-sdk-python/). Make sure to use the correct version of the ank-sdk-python that fits to the Ankaios version in use.

## Deploying the fleet connector

If you have not yet installed Ankaios, please follow the instructions [here](https://eclipse-ankaios.github.io/ankaios/latest/usage/installation/).
The following examples assume that the installation script has been used with the default options.

The fleet connector shall run when the vehicle has been started and thus Ankaios has been started. For that reason we add the fleet connector to start configuration for Ankaios. Modify `/etc/ankaios/state.yaml` to contain:

```yaml title="/etc/ankaios/state.yaml"
apiVersion: v0.1
workloads:
  fleetconnector:
    runtime: podman
    agent: agent_A
    restart: false
    controlInterfaceAccess:
      allowRules:
        - type: StateRule
          operation: ReadWrite
          filterMask:
            - "*"
    restartPolicy: NEVER
    runtimeConfig: |
      image: ghcr.io/eclipse-ankaios/fleet-connector:0.5.0
      commandOptions: [ "--net=host", "-e", "VIN=1"]
```

As the fleet connector needs to access the Ankaios control interface we need to allow that with the `controlInterfaceAccess` section in the manifest. See the [reference documentation](https://eclipse-ankaios.github.io/ankaios/latest/reference/_ankaios.proto/#controlinterfaceaccess) for more information on that.

Now we start Ankaios with:

```shell
sudo systemctl start ank-server ank-agent
```

And we check that the fleet connector is up and running:

```shell
ank -k get workloads
```

## Remote installation of a vehicle data sender

Now we want to use the fleet connector to remotely install a new containerized workload on the vehicle. The workload is called `vehicle-data-sender`. It will send a (random) speed limit in the cloud to the MQTT broker.

First we need to create a manifest and name that file `vehicle-data-sender.yaml`:

```yaml title="vehicle-data-sender.yaml"
apiVersion: v0.1
workloads:
  vehicle-data-sender:
    runtime: podman
    restartPolicy: NEVER
    agent: agent_A
    configs:
      c: vehicle-data-sender-config
    runtimeConfig: |
      image: ghcr.io/eclipse-ankaios/vehicle-data-sender:0.1.0
      commandOptions:
        - "--net=host"
        {{#each c.env}}
        - "-e"
        - "{{key}}={{value}}"
        {{/each}}
configs:
  vehicle-data-sender-config:
    env:
      - key: TOPIC
        value: vehicle/1/sensors/speed
      - key: INTERVAL
        value: "1"
```

Then we send this file via MQTT to the topic `vehicle/1/manifest/apply/req`:

```shell
TOPIC=vehicle/1/manifest/apply/req
FILE=vehicle-data-sender.yaml
podman run --rm --net=host -v $PWD/$FILE:/$FILE docker.io/eclipse-mosquitto mosquitto_pub -h localhost -t "$TOPIC" -f $FILE
```

The fleet connector will receive this message and use the Ankaios control interface to apply this manifest. When looking at the previous window in which we subscribed to MQTT topics, we can see that we receive messages from vehicle data sender like:

```text
vehicle/1/sensors/speed 1
vehicle/1/sensors/speed 2
vehicle/1/sensors/speed 3
...
```

We can also remotely request the workload states from the fleet connector by sending a message to the topic `vehicle/1/state/req`. Our fleet connector requires the object field mask in the message to we send `["workloadStates"]`:

```shell
TOPIC=vehicle/1/state/req
MSG='["workloadStates"]'
podman run --rm --net=host docker.io/eclipse-mosquitto mosquitto_pub -h localhost -t "$TOPIC" -m "$MSG"
```

In the windows with the MQTT subcription we can see the state arrive as JSON object using the topic `vehicle/1/state/resp`.

