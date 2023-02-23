#!/usr/bin/env bash

set -x
set -eo pipefail

if ! [ -x "$(command -v docker)" ]; then
  echo >&2 "Error: docker is not installed"
  echo >&2 "Install docker for your system"
  exit 1
fi

REDIS_NAME="redislocal"
NET_NAME="backdrop-dev-net"
CONTAINER_TAG="backdrop"

# Create new network if one does not exist yet.
docker network inspect ${NET_NAME} >/dev/null 2>&1 \
  || docker network create ${NET_NAME}

# TODO: Switch to new redis instance in case of network switch

# Run new redis container if there is none yet.
if [ ! "$(docker ps -a -q -f name=$REDIS_NAME)" ]; then
  echo >&2 "Missing redis instance. Starting new one."
  docker run \
    --name $REDIS_NAME \
    --network $NET_NAME \
    -p "127.0.0.1:6379:6379" \
    -d \
    redis:latest redis-server \
    --appendonly yes
fi

# Run new redis container if there is one which is exited.
if [ "$(docker ps -aq -f status=exited -f name=$REDIS_NAME)" ]; then
  echo >&2 "Exited redis instance found. Restarting new one."
  docker rm $REDIS_NAME

  docker run \
    --name $REDIS_NAME \
    --network $NET_NAME \
    -p "127.0.0.1:6379:6379" \
    -d \
    redis:latest redis-server \
    --appendonly yes
fi

echo >&2 "Redis is up and ready"

# Build app
docker build \
  --tag $CONTAINER_TAG \
  --file Dockerfile .

# Run app with optionally pretty printed logs.
if ! [ -x "$(command -v bunyan)" ]; then
  echo >&2 "Warning: bunyan formatter is not installed"
  echo >&2 "  Run 'cargo install bunyan' to install it"
  # Run without pretty printing
  docker run \
    -p 8000:8000 \
    --network $NET_NAME \
    --env APP_REDIS_URI="redis://${REDIS_NAME}" \
    $CONTAINER_TAG
else
  # Run with pretty printing
  docker run \
    -p 8000:8000 \
    --network $NET_NAME \
    --env APP_REDIS_URI="redis://${REDIS_NAME}" \
    $CONTAINER_TAG \
    | bunyan
fi

