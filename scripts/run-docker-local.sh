#!/usr/bin/env bash

docker build --tag  backdrop-devel --file Dockerfile .
docker run -p 8000:8000 backdrop-devel
