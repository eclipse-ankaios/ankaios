FROM docker.io/alpine:3.18.4 as base
RUN apk update && apk add --update-cache \
    # Development tools
    python3 \
    bash \
    git \
    curl \
    && rm -rf /var/cache/apk/* \
    && python3 -m ensurepip && pip3 install protobuf==3.20.2

COPY ankaios_api/proto/ank_base.proto ankaios_api/proto/control_api.proto .
COPY examples/tools/setup_python_sdk.sh .
RUN if [[ -z "$PYTHON_SDK_BRANCH" ]] ; then ./setup_python_sdk.sh ; else ./setup_python_sdk.sh --sdk-source github --sdk-branch "$PYTHON_SDK_BRANCH" --proto-source local --proto-path . ; fi

COPY examples/python_sdk_interactive /ankaios
ENV ANKAIOS_VERSION=1.0.0

ENTRYPOINT ["python3", "-u", "/ankaios/src/main.py"]
