#!/bin/bash

{
for i in {0..999}
do
cat <<EOF
---
kind: ElasticsearchUser
apiVersion: eeops.io/v
metadata:
  name: demo-$i
  namespace: default
spec:
  username: server-$i
  secretRef: server-elastic-$i
  prefixes:
    - blog-articles
  permissions: Create
EOF
done
} | time microk8s kubectl apply -f
