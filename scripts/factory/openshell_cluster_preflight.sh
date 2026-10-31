#!/usr/bin/env bash
set -euo pipefail

run_id="${1:?GitHub Actions run ID is required}"
[[ "$run_id" =~ ^[0-9]{1,20}$ ]] || { echo 'invalid run id' >&2; exit 2; }
ns="openshell-preflight-${run_id}"
cleanup() { kubectl delete namespace "$ns" --wait=false >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo '== Kubernetes and Helm versions =='
kubectl version --output=yaml
helm version --short
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

