# External Elasticsearch Operator

This operator automatically manages Elasticsearch roles and users
for your applications running in Kubernetes.
So one application has a user to write to index `blogs-*` and `search-*`,
while the other can only read from `search-articles`.
It does not manage the Elasticsearch cluster itself,
but uses the API to provision resources. It therefore requires
an admin user like `elastic`.
For performance information and potential footguns see
[notes and considerations](#notes-and-considerations) for details.

## Installation of the Operator
The operator is currently namespaced. Meaning the operator
has to be in the same namespace as the target CRs.
This also means, one Elasticsearch instance can be provisioned per namespace.

We will install the operator in the default namespace.


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
PACKAGE_URL="https://github.com/julianbuettner/ext-elasticsearch-operator/raw/main/helm-repo/ext-elasticsearch-operator-1.0.5.tgz"
helm install eeops "$PACKAGE_URL" --set environmentVariablesSecretRef=eeops-env
```
Use `--set loglevel=debug` to get more info. Generally, only changes are logged
at info level, while re-checking leaves debug logs.

## Example Custom Resource
Make sure the username and secret ref are unique.
Otherwise values will override constantly.
```yaml
kind: ElasticsearchUser
apiVersion: eeops.io/v
metadata:
  name: demo
  namespace: default
spec:
  username: server
  secretRef: server-elastic
  prefixes:
    - blog-articles
  permissions: Create
```

The secret `foobar` should be created within around a second
and has the following keys:
```bash
ELASTICSEARCH_PASSWORD=randomly-generated
ELASTICSEARCH_URL=as-specified-for-the-controller
ELASTICSEARCH_USERNAME=as-specified-in-the-crd
```

## Notes and Considerations
### General Notes and Footguns
- The secrets are deleted, if the ElasticsearchUser are deleted.
- The operator fetches the role and userdata to check if they match
the desired state. It also does a login to test the credentials.
Only in case of a mismatch, put/post/patch requests are made.
- Currently, all ElasticsearchUsers are re-checked every 15min.
- If the `secretRef` is changed, the old secret is not removed automatically.
A new secret with a new password is generted. The old one does not work anymore.
- Manually changing the password of a secret is supported. It is applied immediately.
- Already existing secrets will be patched and still deleted if the CR is deleted.
- Running multiple opertor might result in complications and has no benefits.

### Deletion
An ElasticsearchUser custom resource can't be deleted if the operator is stopped. To make sure
no user deletion is missed, the operator uses so called finalizers.
To force the immediate deletion of an ElasticsearchUser,
delete the `.metadata.finalizer` entries manually. Then the object is deletable.

### Performance and Resources
In idle or with little usage, the operator uses around 2MiB to 3MiB memory and
between 0 and 1 mCores (milli core). If you use less than a few dozend
ElasticsearchUsers, there should be no performance concernces.

The operator can handle dozens of patches per second, so performance
should not be an issue, even for big clusters with lots of applications
using Elasticsearch. 1K CR updates did need around 40s to be applied
on my testing one node cluster.
However, a copy of all custom resources are kept in RAM and are recycled
a few times an hour, to ensure Elasticsearch is configured correctly.
So, with every 1K of new ElasticsearchUser objects, around 20MiB more are used.
Also keep in mind, that at every restart of the controller, all CRs are
reconciled.
