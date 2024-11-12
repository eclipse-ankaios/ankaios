FROM docker.io/alpine:3.18.4 as base
RUN apk update && apk add --update-cache \
    # Development tools
    python3 \
    && rm -rf /var/cache/apk/* \
    && python3 -m ensurepip && pip3 install protobuf==3.20.2

FROM base as dev

RUN apk update && apk add --update-cache \
    # Development tools
    protoc \
    && rm -rf /var/cache/apk/*

COPY api/proto/ank_base.proto api/proto/control_api.proto /usr/local/lib/ankaios/
RUN protoc --python_out=/usr/local/lib/ankaios/ --proto_path=/usr/local/lib/ankaios/ ank_base.proto control_api.proto && touch /usr/local/lib/ankaios/__init__.py

# prod stage
FROM base
ENV PYTHONPATH="${PYTHONPATH}:/usr/local/lib/ankaios"
COPY --from=dev /usr/local/lib/ankaios /usr/local/lib/ankaios
COPY examples/python_control_interface /ankaios
ENV ANKAIOS_VERSION=0.5.0

ENTRYPOINT ["python3", "-u", "/ankaios/src/main.py"]
