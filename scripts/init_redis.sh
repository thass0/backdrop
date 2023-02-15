#!/usr/bin/env bash

set -x
set -eo pipefail

running_container=$(docker ps --filter 'name=redis' --format '{{.ID}}')
if [[ -n $running_container ]]; then
  echo >&2 "there is a redis container already running, kill it with"
  echo >&2 "  docker kill ${running_container}"
  exit 1
fi

# Launch redis using docker
docker run \
  -p "6379:6379" \
  -d \
  --name "redis_$(date '+%s')" \
  redis:6

>&2 echo "Redis is ready to go!"
