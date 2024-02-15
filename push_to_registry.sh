#!/bin/bash


# Fuck docker for wanting money to host open source container images

set -e

TAG=$(cargo metadata --no-deps | jq -r .packages[0].version )

echo Publish current version as $TAG

podman build -t julianbuettner1/ext-elasticsearch-operator:$TAG .
podman push julianbuettner1/ext-elasticsearch-operator:$TAG
