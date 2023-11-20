FROM ghcr.io/eclipse-ankaios/app-ankaios-dev:latest as dev

ENV PYTHONPATH="${PYTHONPATH}:/usr/local/lib/ankaios"

RUN apt-get update && apt-get -y install \
    # Development tools
    python3 \
    python3-pip \
    && \
    rm -rf /var/lib/apt/lists/*

RUN pip3 install protobuf==3.20.2

RUN protoc --python_out=/usr/local/lib/ankaios/ --proto_path=/usr/local/lib/ankaios/ ankaios.proto && touch /usr/local/lib/ankaios/__init__.py

# prod stage
FROM docker.io/library/python:3.12-slim-bullseye
ENV PYTHONPATH="${PYTHONPATH}:/usr/local/lib/ankaios"
RUN pip3 install protobuf==3.20.2
COPY --from=dev /usr/local/lib/ankaios /usr/local/lib/ankaios
COPY src/ /ankaios/src

ENTRYPOINT ["python3", "-u", "/ankaios/src/main.py"]
