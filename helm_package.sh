#!/bin/bash

set -e

cd "$(dirname $0)"
cd helm-repo

helm package ../helm-chart
helm repo index .
echo Packaged chart and refreshed index. Done.
