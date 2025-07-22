FROM docker.io/alpine:3.18.4 as base
RUN apk update && apk add --update-cache \
    # Development tools
    python3 \
    && rm -rf /var/cache/apk/* \
    && python3 -m ensurepip && pip3 install protobuf==3.20.2

RUN apk update && apk add --update-cache \
    bash git curl \
    && rm -rf /var/cache/apk/*

COPY examples/python_sdk/setup_python_sdk.sh .

# This will setup the python sdk from the "61_log_collection" branch
# and will use the proto files from the "feature/90_provide_access_to_workload_logs" branch.
# Adjust the params as needed.
RUN ./setup_python_sdk.sh --sdk-source github --sdk-branch 61_log_collection --proto-source branch --proto-branch feature/90_provide_access_to_workload_logs

COPY examples/python_sdk /ankaios
ENV ANKAIOS_VERSION=0.6.0-pre

ENTRYPOINT ["python3", "-u", "/ankaios/src/main.py"]
