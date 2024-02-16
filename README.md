# External Elasticsearch Operator

This operator automatically manages Elasticsearch roles and users
for your applications running in Kubernetes.
It does not manage the Elasticsearch cluster itself,
but uses the API to provision resources.


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

PACKAGE_URL="https://github.com/julianbuettner/ext-elasticsearch-operator/raw/main/helm-repo/ext-elasticsearch-operator-0.1.0.tgz"
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


