# External Elasticsearch Operator

This operator automatically manages Elasticsearch roles and users
for your applications running in Kubernetes.
It does not manage the Elasticsearch cluster itself,
but uses the API to provision resources.
The operator is rather lightweight. Scroll to [notes](#resources) for details.

## Installation of the Operator
The operator is currently namespaced. Meaning the pod
has to be in the same namespace as the target CRDs.
This also means, one Elasticsearch instance per namespace.
We well install the operator in the default namespace.


```bash
ELASTIC_PASSWORD=mypass
ELASTIC_URL=http://elastic:9200
ELASTIC_USERNAME=elastic

cat << EOF | kubectl apply -f -
kind: Secret
apiVersion: v1
metadata:
  name: eeops-env
  namespace: default
data:
  ELASTIC_PASSWORD: $(echo -n $ELASTIC_PASSWORD | base64)
  ELASTIC_URL: $(echo -n $ELASTIC_URL | base64)
  ELASTIC_USERNAME: $(echo -n $ELASTIC_USERNAME | base64 )
type: Opaque
EOF

# Note: check directory for newer versions, I might have forgotten to update
# the version in the URL.
PACKAGE_URL="https://github.com/julianbuettner/ext-elasticsearch-operator/raw/main/helm-repo/ext-elasticsearch-operator-1.0.0.tgz"
helm install eeops "$PACKAGE_URL" --set environmentVariablesSecretRef=eeops-env
```

## Example Custom Resource
```yaml
apiVersion: eeops.io/v1
kind: ElasticsearchUser
metadata:
  name: demo
spec:
  permissions: Write  # Read, Write, Create
  prefixes:
    - application1-  # Use indices "application1-*"
  secret_ref: foobar  # Creates this secret
  username: foome  # ensure the username is unique
```

The secret `foobar` should be created within around a second
and has the following keys:
```bash
ELASTICSEARCH_PASSWORD=randomly-generated
ELASTICSEARCH_URL=as-specified-for-the-controller
ELASTICSEARCH_USERNAME=as-specified-in-the-crd
```

## Notes and considerations
### Resources
In idle, the operator uses around 2MiB to 3MiB and
between 0 and 1 mCores (milli core).
On load (values per second rather than per minute) these values
are expected to go up slighly.

### Performance
Currently, all operations are performed sequentially.
If you expect to have many operations / CR updates
per second for an extended period of time,
please open an issue. Then I will invest time into performance.

