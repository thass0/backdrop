#!/usr/bin/env bash

set -x
set -eo pipefail

# Check whether docker is installed on the host machine.
if ! [ -x "$(command -v docker)" ]; then
  echo >&2 "Error: Docker is not installed."
  echo >&2 "Please install Docker for your system (https://docs.docker.com/get-docker/)."
  echo >&2 "Try running this script again after you have successfully installed Docker."
  exit 1
fi

# Check if docker-desktop in running
DOCKER_ACTIVE=$(systemctl --user --no-pager status docker-desktop | grep "active (running)")
if ! [[ -n $DOCKER_ACTIVE ]]; then
  echo >&2 "docker-desktop is not active. Starting docker-desktop"
  systemctl --user start docker-desktop
  echo >&2 "Starting docker-desktop might take a while. Please run this script again once Docker is ready."
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

echo >&2 "Running backdrop on http://localhost:8000"

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
