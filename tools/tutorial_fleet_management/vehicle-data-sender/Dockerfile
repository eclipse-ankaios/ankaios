FROM python:3.12-slim-bookworm

WORKDIR /usr/src/app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

COPY vehicle-data-sender.py ./

CMD [ "python", "-u", "./vehicle-data-sender.py" ]
