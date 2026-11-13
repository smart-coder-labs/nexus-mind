"""Runner -> backend call failures: typed, named and safe to report.

Kept apart from runner.py so execution/delivery modules can classify these errors
without importing runner.py a second time when it runs as __main__.
"""
import json
import urllib.error

# Contract version of the runner protocol; sent as a capability so a backend can
# refuse an incompatible runner before leasing it work.
RUNNER_CONTRACT = 1
TRANSIENT_STATUSES = (408, 429, 500, 502, 503, 504)


class ControlPlaneError(OSError):
    """A runner -> backend call failed. Carries only non-secret identifiers:
    method, route template (no IDs or query), status, backend code, request id."""

    def __init__(self, method, route, request_id, status=None, code='', json_body=False, reason=''):
        super().__init__('control plane request failed')
        self.method, self.route, self.request_id = method, route, request_id
        self.status, self.backend_code, self.json_body, self.reason = status, code, json_body, reason

    @property
    def transient(self):
        return self.status is None or self.status in TRANSIENT_STATUSES


def from_http_error(error, method='?', route='?', request_id=None):
    """Wrap an HTTPError, reading its code only when the body is a JSON object.
    The body itself is never kept: proxies may echo request details."""
    code, json_body = '', False
    try:
        payload = json.loads(error.read(65536))
        json_body = isinstance(payload, dict)
        code = str(payload.get('code', ''))[:100] if json_body else ''
    except (ValueError, OSError):
        pass
    finally:
        error.close()
    return ControlPlaneError(method, route, request_id, error.code, code, json_body)


def route_template(path):
    """`/runs/<id>/steps/<id>/lease` -> `/runs/:run/steps/:step/lease`."""
    parts = path.split('?', 1)[0].split('/')
    names = {'runs': ':run', 'steps': ':step', 'workspaces': ':workspace', 'sources': ':index'}
    return '/'.join(names[parts[i - 1]] if i and parts[i - 1] in names and part else part
                    for i, part in enumerate(parts))


def idempotent(method, path):
    # Retrying these cannot repeat agent work: leases and events are fenced by the
    # lease token, artifacts are digest-addressed. Claim, complete (journaled and
    # reconciled), model calls and publication are never retried here.
    return method == 'GET' or path.rsplit('/', 1)[-1] in ('lease', 'events', 'artifacts')


CONTROL_PLANE_SOLUTIONS = {
    'unavailable': 'El backend no respondió (error del ingress: reinicio, desalojo del pod o sobrecarga). No es un fallo del código: comprueba que el backend esté disponible y reintenta la fase.',
    'network': 'El runner no pudo conectar con el backend. Revisa conectividad y disponibilidad del API y reintenta la fase; no es un fallo del código.',
    'version': 'El backend no admite esta llamada del runner: sus versiones de contrato no coinciden. Actualiza el componente atrasado antes de reintentar.',
    'size': 'La petición del runner superó el tamaño admitido por el backend. Revisa el artefacto o evento de esta fase.',
    'quota': 'Se alcanzó una cuota del backend para esta ejecución. Los logs ya no bloquean la fase; si se repite, revisa el volumen de eventos.',
    'lease': 'El lease de la fase expiró o pertenece a otro intento. Comprueba el estado de la ejecución antes de reintentar.',
    'credential': 'El backend rechazó la credencial del runner. Revisa su registro o rotación.',
    'invalid_event': 'El backend rechazó un evento del runner. Actualiza el runner para usar el contrato de eventos admitido; no es un fallo de los tests y no se reintentará automáticamente.',
    'other': 'El backend rechazó una llamada del runner. Busca el request_id en los logs del backend para ver la causa.',
}


def control_plane_diagnostic(error):
    """Name a failed runner -> backend call. Always holds: an unreachable control
    plane is not a code defect and must not trigger an automatic repair cycle."""
    if isinstance(error, urllib.error.HTTPError):
        error = from_http_error(error)
    status, code = error.status, error.backend_code
    if code:
        name = code
    elif status is None:
        name = 'adapter_network_' + (error.reason or 'error')
    else:
        name = 'adapter_http_' + str(status)
    if code in ('route_not_found', 'method_not_allowed', 'runner_version_unsupported') or (not error.json_body and status in (404, 405)):
        kind = 'version'
    elif code == 'invalid_event':
        kind = 'invalid_event'
    elif code == 'body_too_large' or status == 413:
        kind = 'size'
    elif code == 'event_quota' or status == 429:
        kind = 'quota'
    elif code == 'lease_conflict':
        kind = 'lease'
    elif status == 401:
        kind = 'credential'
    elif status is None or status == 408:
        kind = 'network'
    elif status in (500, 502, 503, 504) and not error.json_body:
        kind = 'unavailable'
    else:
        kind = 'other'
    return {'error': name, 'exception_type': 'ControlPlaneError', 'request': error.method + ' ' + error.route if error.route != '?' else None,
            'http_status': status, 'backend_code': code or None, 'response_json': error.json_body,
            'request_id': error.request_id, 'retryable': error.transient,
            'solution': CONTROL_PLANE_SOLUTIONS[kind], 'hold': True}
