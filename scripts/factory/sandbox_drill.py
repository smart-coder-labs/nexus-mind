"""Adversarial drill for factory task pods (F1 task 6.1).

Runs INSIDE a task pod and tries what a malicious repository or prompt would
try. Every attack must fail. Prints one JSON line per check and exits non-zero
if any attack succeeded (or if nothing was checked).

Run through the executor's wrapper so the process limit applies:

    kubectl -n nexusmind-sandbox exec -i <task-pod> -- \\
        prlimit --nproc=512:512 -- python3 - < scripts/factory/sandbox_drill.py

Run it in both pods of a run. In the agent pod it checks confinement, network
and proxy denials. In the commands pod (repository code, no ANTHROPIC_BASE_URL)
it also checks that no process anywhere in the pod holds a token that reaches
Claude or NexusMind, and that the registry-only token is refused on those routes.

Optional: DRILL_REGISTRY_TOKEN=<registry-only run token> checks that token from
the agent pod too.

Standard library only: the task image has python3 but no extra packages.
"""

import base64
import json
import os
import re
import signal
import socket
import sys
import time
import urllib.parse

CONNECT_TIMEOUT = 3

# Values that are real credentials wherever they appear. Run tokens (v2.…) and
# the Claude placeholder are expected in the pod and are not listed.
CREDENTIAL_PATTERNS = [
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}"),
    re.compile(r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
    re.compile(r"\bnm_[A-Za-z0-9_]{16,}"),
    # URL with user:password, except the proxy's `run:<run token>` userinfo.
    re.compile(r"://(?!run:v2\.)[^/\s:@]+:[^/\s@]+@"),
]
# Names that must not exist in the pod at all, whatever their value.
FORBIDDEN_NAMES = {
    "ANTHROPIC_API_KEY",
    "NEXUSMIND_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "DATABASE_URL",
    "BACKUP_DATABASE_URL",
    "JEV_API_KEY",
    "TYPESAFE_API_KEY",
    "FACTORY_PROXY_SIGNING_KEY",
}


def confinement_problems(status):
    """Weaknesses in /proc/self/status: root, capabilities, no_new_privs, seccomp."""
    fields = {}
    for line in status.splitlines():
        name, _, value = line.partition(":")
        fields[name.strip()] = value.split()
    if not fields.get("Uid") or not fields.get("CapEff"):
        return ["status_unreadable"]
    problems = []
    if "0" in fields["Uid"]:
        problems.append("runs_as_root")
    if int(fields["CapEff"][0], 16) != 0:
        problems.append("has_capabilities")
    if fields.get("NoNewPrivs", ["0"])[0] != "1":
        problems.append("privilege_escalation_allowed")
    if fields.get("Seccomp", ["0"])[0] != "2":
        problems.append("no_seccomp")
    return problems


def uid_map_is_isolated(uid_map):
    """True when in-pod UIDs map to a host range other than the host's own."""
    rows = [line.split() for line in uid_map.splitlines() if line.strip()]
    if not rows:
        return False
    return all(len(row) == 3 and row[1] != "0" for row in rows)


def leaked_credentials(env):
    """Names of environment variables that hold a real credential."""
    leaked = set()
    for name, value in env.items():
        if name in FORBIDDEN_NAMES or any(p.search(value) for p in CREDENTIAL_PATTERNS):
            leaked.add(name)
    return sorted(leaked)


# File names that hold credentials or the worker's data wherever they appear.
SENSITIVE_FILES = {
    ".credentials.json",
    ".claude.json",
    ".env",
    ".git-credentials",
    ".netrc",
    "hosts.yml",
    "config.json",
}
SENSITIVE_SUFFIXES = (".db", ".db-wal", ".sqlite", ".pem", ".key")


def path_exposure(path, mountinfo):
    """Why `path` exposes something to the pod: it is a mount point, or it holds
    credential or data files. An empty directory baked into the image is safe."""
    mount_points = {line.split()[4] for line in mountinfo.splitlines() if len(line.split()) > 4}
    if os.path.abspath(path) in mount_points:
        return ["mounted"]
    if not os.path.exists(path):
        return []
    found = []
    for directory, _, files in os.walk(path):
        for name in files:
            if name in SENSITIVE_FILES or name.endswith(SENSITIVE_SUFFIXES):
                found.append(os.path.relpath(os.path.join(directory, name), path))
    return sorted(found)


class Result:
    def __init__(self, name, blocked, detail):
        self.name = name
        self.blocked = blocked
        self.detail = detail


def exit_code(results):
    if not results or not all(result.blocked for result in results):
        return 1
    return 0


# ---------------------------------------------------------------- attacks


def read(path):
    try:
        with open(path) as handle:
            return handle.read()
    except OSError:
        return ""


def check_process():
    problems = confinement_problems(read("/proc/self/status"))
    yield Result("proc:confinement", not problems, problems or "non-root, no caps, no_new_privs, seccomp")
    yield Result("proc:user_namespace", uid_map_is_isolated(read("/proc/self/uid_map")), read("/proc/self/uid_map").strip())
    visible = [entry for entry in os.listdir("/proc") if entry.isdigit()]
    init = read("/proc/1/cmdline").replace("\0", " ").strip()
    # Own PID namespace: PID 1 is the pod's `sleep infinity`, not the host init.
    yield Result("proc:pid_namespace", "sleep" in init and len(visible) < 100, f"pid1={init!r} visible={len(visible)}")


def check_files():
    token = "/var/run/secrets/kubernetes.io/serviceaccount/token"
    yield Result("fs:service_account_token", not os.path.exists(token), token)
    mountinfo = read("/proc/self/mountinfo")
    # The worker's volume paths exist as empty directories in the image; what
    # matters is that nothing is mounted there and no credential or data is inside.
    for path in ["/data", "/claude-home", "/app"]:
        exposed = path_exposure(path, mountinfo)
        yield Result(f"fs:{path}", not exposed, exposed or "no mount, no credential or data files")
    for directory in ["/etc", "/usr/bin", "/app"]:
        target = os.path.join(directory, ".drill-write")
        try:
            with open(target, "w") as handle:
                handle.write("x")
            os.unlink(target)
            yield Result(f"fs:write:{directory}", False, "writable")
        except OSError as error:
            yield Result(f"fs:write:{directory}", True, error.strerror)


def process_environs():
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                with open(f"/proc/{entry}/environ", "rb") as handle:
                    yield handle.read()
            except OSError:
                continue


def check_environment():
    environs = list(process_environs())
    leaked = set()
    for environ in environs:
        env = dict(
            item.decode(errors="replace").partition("=")[::2]
            for item in environ.split(b"\0")
            if item
        )
        leaked.update(leaked_credentials(env))
    yield Result("env:credentials", not leaked, sorted(leaked) or f"none in {len(environs)} processes")
    # In a commands pod (no agent routes in its own environment) no process may
    # hold a token that reaches Claude or NexusMind: any process can read PID 1's.
    if "ANTHROPIC_BASE_URL" not in os.environ:
        found = upstream_tokens_in(environs)
        yield Result("env:no_agent_token_in_commands_pod", found == 0, f"{found} processes")


def tcp(host, port):
    """Result of a direct connection: only an established connection is a success."""
    try:
        with socket.create_connection((host, port), timeout=CONNECT_TIMEOUT):
            return False, "connected"
    except socket.gaierror as error:
        # The pod's resolver works (kube-dns is allowed): a name that does not
        # resolve proves nothing about reachability.
        return False, f"inconclusive: dns {error}"
    except (socket.timeout, TimeoutError):
        return True, "timeout"
    except OSError as error:
        return True, error.strerror or str(error)


def check_network():
    targets = [
        ("internet", "1.1.1.1", 443),
        ("anthropic_direct", "api.anthropic.com", 443),
        ("github_direct", "github.com", 443),
        ("kubernetes_api_dns", "kubernetes.default.svc.cluster.local", 443),
        ("kubernetes_api_ip", "10.43.0.1", 443),
        ("backend_service", "nexusmind-backend.nexusmind.svc.cluster.local", 80),
        ("backend_service_3000", "nexusmind-backend.nexusmind.svc.cluster.local", 3000),
        ("cloud_metadata", "169.254.169.254", 80),
    ]
    for name, host, port in targets:
        blocked, detail = tcp(host, port)
        yield Result(f"net:{name}", blocked, detail)


def proxy_parts(env):
    """Proxy host, port and run token, from HTTPS_PROXY (present in every pod)."""
    https_proxy = urllib.parse.urlsplit(env.get("HTTPS_PROXY", ""))
    token = urllib.parse.unquote(https_proxy.password or "")
    return https_proxy.hostname, https_proxy.port or 80, token


def upstream_tokens_in(environs):
    """How many process environments hold a token for a credentialed upstream."""
    return sum(
        1
        for environ in environs
        if b"/anthropic" in environ or b"/nexusmind" in environ
    )


def proxy_status(host, port, request):
    try:
        with socket.create_connection((host, port), timeout=CONNECT_TIMEOUT) as conn:
            conn.sendall(request.encode())
            conn.settimeout(10)
            head = conn.recv(64).decode(errors="replace")
    except OSError as error:
        return None, error.strerror or str(error)
    match = re.match(r"HTTP/1\.[01] (\d{3})", head)
    return (int(match.group(1)) if match else None), head.split("\r\n", 1)[0]


def basic(token):
    return base64.b64encode(f"run:{token}".encode()).decode()


def check_proxy():
    host, port, token = proxy_parts(os.environ)
    if not host or not token:
        yield Result("proxy:configured", False, "HTTPS_PROXY missing")
        return
    cases = [
        ("connect_without_token", "CONNECT evil.example:443 HTTP/1.1\r\nHost: evil.example:443\r\n\r\n", 407),
        (
            "connect_off_allowlist",
            f"CONNECT evil.example:443 HTTP/1.1\r\nHost: evil.example:443\r\nProxy-Authorization: Basic {basic(token)}\r\n\r\n",
            403,
        ),
        (
            "connect_metadata",
            f"CONNECT 169.254.169.254:443 HTTP/1.1\r\nHost: x\r\nProxy-Authorization: Basic {basic(token)}\r\n\r\n",
            403,
        ),
        (
            "anthropic_non_inference_path",
            f"GET /r/{token}/anthropic/api/oauth/profile HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n",
            403,
        ),
        (
            "forged_token",
            f"POST /r/v2.x.y.9999999999.{'0' * 64}/anthropic/v1/messages HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            403,
        ),
    ]
    registry = os.environ.get("DRILL_REGISTRY_TOKEN") or (
        token if "ANTHROPIC_BASE_URL" not in os.environ else None
    )
    if registry:
        cases.append(
            (
                "registry_token_to_anthropic",
                f"POST /r/{registry}/anthropic/v1/messages HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                403,
            )
        )
    for name, request, expected in cases:
        status, line = proxy_status(host, port, request)
        yield Result(f"proxy:{name}", status == expected, line)


def check_fork_limit(attempts=600):
    """The process limit must stop a fork bomb well before the node's limit."""
    children = []
    try:
        for _ in range(attempts):
            try:
                pid = os.fork()
            except OSError:
                break
            if pid == 0:
                time.sleep(30)
                os._exit(0)
            children.append(pid)
    finally:
        for pid in children:
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            except OSError:
                pass
    yield Result("proc:fork_limit", len(children) < attempts, f"forked {len(children)} of {attempts}")


def main():
    results = []
    for check in [check_process, check_files, check_environment, check_network, check_proxy, check_fork_limit]:
        for result in check():
            results.append(result)
            print(json.dumps({"check": result.name, "blocked": result.blocked, "detail": result.detail}), flush=True)
    failed = [result.name for result in results if not result.blocked]
    print(json.dumps({"summary": "pass" if exit_code(results) == 0 else "fail", "checks": len(results), "failed": failed}))
    return exit_code(results)


if __name__ == "__main__":
    sys.exit(main())
