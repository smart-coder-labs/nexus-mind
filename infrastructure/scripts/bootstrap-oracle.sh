#!/usr/bin/env bash
# One-time preparation of the oracle box (Oracle Cloud Ampere A1, Ubuntu 22.04)
# so the CI deploy + Caddy TLS can work. Run ON THE BOX as the `ubuntu` user:
#
#   ssh oracle 'bash -s' < infrastructure/scripts/bootstrap-oracle.sh
#
# It is idempotent. It does NOT open the Oracle Cloud *Security List / NSG* — that
# lives in the OCI web console and MUST be done by you: add ingress rules allowing
# TCP 80 and 443 from 0.0.0.0/0 to this instance's subnet. Without BOTH the OCI
# security list AND the host iptables below, Let's Encrypt (and all HTTPS) fail.
set -euo pipefail

echo "==> Checking Docker..."
if ! command -v docker >/dev/null 2>&1; then
  echo "ERROR: docker not found. Install Docker Engine first." >&2
  exit 1
fi
docker --version

# Oracle Cloud's stock Ubuntu image ships an iptables INPUT chain that ends in a
# REJECT rule, so ports opened in the Security List are still blocked on the host.
# Insert explicit ACCEPTs for 80/443 ABOVE that REJECT, then persist them.
echo "==> Opening host firewall for 80/443 (idempotent)..."
sudo apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq iptables-persistent netfilter-persistent >/dev/null

for port in 80 443; do
  if ! sudo iptables -C INPUT -p tcp --dport "$port" -j ACCEPT 2>/dev/null; then
    # Insert before the trailing REJECT (rule 6 on the stock image); fall back to
    # appending if the chain layout differs.
    sudo iptables -I INPUT 6 -p tcp --dport "$port" -j ACCEPT 2>/dev/null \
      || sudo iptables -A INPUT -p tcp --dport "$port" -j ACCEPT
    echo "  opened tcp/$port"
  else
    echo "  tcp/$port already open"
  fi
done

echo "==> Persisting iptables rules..."
sudo netfilter-persistent save

echo
echo "Host firewall ready. Remaining MANUAL prerequisites (not scriptable here):"
echo "  1. OCI console -> Networking -> Security List: allow ingress TCP 80 and 443."
echo "  2. Cloudflare DNS: create/point api.nexusmind.smartcoderlabs.com only at"
echo "     the CUTOVER step. Caddy uses DNS-01, so the cert is issued beforehand."
echo "  3. Seed the DB with infrastructure/scripts/seed-oracle-db.sh (run locally)."
