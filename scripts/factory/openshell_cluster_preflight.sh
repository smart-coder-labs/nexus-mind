#!/usr/bin/env bash
set -euo pipefail

# k3s configures kubectl via its built-in default, but Helm's Kubernetes client
# does not use that k3s-specific fallback. Share the same cluster config.
if [[ -z "${KUBECONFIG:-}" && -f /etc/rancher/k3s/k3s.yaml ]]; then
  export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
fi

run_id="${1:?GitHub Actions run ID is required}"
[[ "$run_id" =~ ^[0-9]{1,20}$ ]] || { echo 'invalid run id' >&2; exit 2; }
action="${2:-preflight}"
[[ "$action" == preflight || "$action" == install-agent-sandbox || "$action" == install-openshell-gateway ]] || {
  echo 'action must be preflight, install-agent-sandbox, or install-openshell-gateway' >&2
  exit 2
}
ns="openshell-preflight-${run_id}"
manifest=""
cleanup() {
  kubectl delete namespace "$ns" --wait=false >/dev/null 2>&1 || true
  [[ -z "$manifest" ]] || rm -f "$manifest"
}
trap cleanup EXIT

echo '== Kubernetes and Helm versions =='
k8s_version="$(kubectl version -o json)"
printf '%s\n' "$k8s_version" | python3 -c '
import json, re, sys
version = json.load(sys.stdin)["serverVersion"]
major = int(re.match(r"\d+", version["major"]).group())
minor = int(re.match(r"\d+", version["minor"]).group())
print(f"Kubernetes server {major}.{minor}")
if (major, minor) < (1, 29):
    raise SystemExit("OpenShell Kubernetes driver requires Kubernetes 1.29+")
'
helm_version="$(helm version --short)"
printf 'Helm %s\n' "$helm_version"
[[ "$helm_version" == v3.* ]] || { echo 'OpenShell gateway requires Helm 3.x' >&2; exit 1; }
echo '== Cluster nodes and CNI hints =='
kubectl get nodes -o wide
kubectl -n kube-system get daemonsets,pods -o wide
echo '== Agent Sandbox API =='
if kubectl get crd sandboxes.agents.x-k8s.io >/dev/null 2>&1; then
  echo 'Agent Sandbox CRD already installed'
else
  echo 'Agent Sandbox CRD is not installed; installer must install it'
fi

echo '== Functional NetworkPolicy enforcement probe =='
kubectl create namespace "$ns"
kubectl run server -n "$ns" --image=busybox:1.36.1 --restart=Never \
  --labels=app=server --command -- sh -c \
  'mkdir -p /www && printf ok >/www/health && httpd -f -p 8080 -h /www'
kubectl run client -n "$ns" --image=busybox:1.36.1 --restart=Never \
  --labels=app=client --command -- sh -c 'sleep 600'
kubectl wait -n "$ns" --for=condition=Ready pod/server pod/client --timeout=180s
server_ip="$(kubectl get pod server -n "$ns" -o jsonpath='{.status.podIP}')"
result="$(kubectl exec -n "$ns" client -- wget -q -T 5 -O - "http://${server_ip}:8080/health")"
[[ "$result" == ok ]] || { echo 'baseline pod connectivity failed' >&2; exit 1; }
kubectl apply -n "$ns" -f - <<'YAML'
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: deny-server-ingress
spec:
  podSelector:
    matchLabels:
      app: server
  policyTypes:
    - Ingress
YAML
sleep 3
if kubectl exec -n "$ns" client -- wget -q -T 5 -O - "http://${server_ip}:8080/health" >/dev/null 2>&1; then
  echo 'CNI does not enforce NetworkPolicy ingress' >&2
  exit 1
fi
kubectl delete networkpolicy deny-server-ingress -n "$ns" --wait=true
sleep 3
result="$(kubectl exec -n "$ns" client -- wget -q -T 5 -O - "http://${server_ip}:8080/health")"
[[ "$result" == ok ]] || { echo 'connectivity did not recover after ingress policy removal' >&2; exit 1; }
kubectl apply -n "$ns" -f - <<'YAML'
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: deny-client-egress
spec:
  podSelector:
    matchLabels:
      app: client
  policyTypes:
    - Egress
YAML
sleep 3
if kubectl exec -n "$ns" client -- wget -q -T 5 -O - "http://${server_ip}:8080/health" >/dev/null 2>&1; then
  echo 'CNI does not enforce NetworkPolicy egress' >&2
  exit 1
fi
echo 'NetworkPolicy enforcement verified separately for pod ingress and egress'

if [[ "$action" == install-agent-sandbox || "$action" == install-openshell-gateway ]]; then
  echo '== Install pinned Agent Sandbox controller v1.0.9 =='
  if ! kubectl get crd sandboxes.agents.x-k8s.io >/dev/null 2>&1; then
    manifest="$(mktemp)"
    curl -fsSL --retry 3 \
      https://github.com/kubernetes-sigs/agent-sandbox/releases/download/v1.0.9/sandbox.yaml \
      -o "$manifest"
    printf '%s  %s\n' \
      3eeb27fa821659593ba92c5454571f08c84abb07096bb9d372c1c6c750f6d7d2 \
      "$manifest" | sha256sum -c -
    kubectl apply -f "$manifest"
  else
    echo 'Agent Sandbox CRD already exists; keeping the installed version'
  fi
  kubectl rollout status deployment/agent-sandbox-controller \
    -n agent-sandbox-system --timeout=180s
  kubectl wait --for=condition=Established \
    crd/sandboxes.agents.x-k8s.io --timeout=60s
  echo 'Agent Sandbox controller and CRD are Ready'
fi

if [[ "$action" == install-openshell-gateway ]]; then
  echo '== Install OpenShell gateway chart v0.1.2 (private ClusterIP) =='
  kubectl get crd sandboxes.agents.x-k8s.io >/dev/null
  kubectl -n agent-sandbox-system rollout status deployment/agent-sandbox-controller --timeout=180s
  kubectl create namespace openshell --dry-run=client -o yaml | kubectl apply -f -
  helm upgrade --install openshell oci://ghcr.io/nvidia/openshell/helm-chart \
    --version 0.1.2 --namespace openshell --wait --timeout 5m \
    --set service.type=ClusterIP \
    --set server.auth.allowUnauthenticatedUsers=true \
    --set server.telemetryEnabled=false
  kubectl -n openshell get service openshell -o jsonpath='{.spec.type}' | grep -qx ClusterIP
  echo '== Allow OpenShell sandboxes to pull the private NexusMind image =='
  kubectl -n nexusmind get secret ghcr-pull -o json | python3 -c '
import json, sys
secret = json.load(sys.stdin)
secret["metadata"]["namespace"] = "openshell"
for key in ("uid", "resourceVersion", "creationTimestamp", "managedFields"):
    secret["metadata"].pop(key, None)
json.dump(secret, sys.stdout)
' | kubectl apply -f -
  kubectl -n openshell patch serviceaccount openshell-sandbox --type=merge \
    -p '{"imagePullSecrets":[{"name":"ghcr-pull"}]}'
  kubectl apply -f - <<'YAML'
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: openshell-worker-ingress
  namespace: openshell
spec:
  podSelector:
    matchLabels:
      app.kubernetes.io/name: openshell
      app.kubernetes.io/instance: openshell
  policyTypes:
    - Ingress
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              kubernetes.io/metadata.name: nexusmind
          podSelector:
            matchLabels:
              app: nexusmind-backend
      - namespaceSelector:
          matchLabels:
            kubernetes.io/metadata.name: openshell
        podSelector:
          matchLabels:
            openshell.ai/component: supervisor
      ports:
        - protocol: TCP
          port: 8080
YAML
  kubectl -n openshell rollout status statefulset/openshell --timeout=180s
  kubectl -n nexusmind rollout status deployment/nexusmind-backend --timeout=180s
  echo '== Register the internal gateway for the autonomous worker =='
  for key in ca.crt tls.crt tls.key; do
    escaped_key="${key//./\\.}"
    kubectl -n openshell get secret openshell-client-tls \
      -o "jsonpath={.data.${escaped_key}}" | base64 -d |
      kubectl -n nexusmind exec -i deploy/nexusmind-backend -c autonomous-worker -- \
        sh -c "umask 077; mkdir -p /claude-home/.config/openshell/gateways/production/mtls; cat > /claude-home/.config/openshell/gateways/production/mtls/$key; chmod 600 /claude-home/.config/openshell/gateways/production/mtls/$key"
  done
  kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- \
    sh -c 'openshell gateway remove production >/dev/null 2>&1 || true; openshell gateway add https://openshell.openshell.svc.cluster.local:8080 --local --name production; openshell gateway select production; openshell status'
  if kubectl exec -n "$ns" server -- nc -z -w 3 openshell.openshell.svc.cluster.local 8080; then
    echo 'OpenShell gateway accepted a connection from outside the Nexus worker pod' >&2
    exit 1
  fi
  echo 'NetworkPolicy denied gateway access from the isolated preflight pod as expected'
  echo 'Private OpenShell gateway is Ready; only the Nexus worker pod may reach TCP/8080.'
fi

