// Credential-free fixture only. A hidden iframe carries bounded geometry and
// origin metadata; it never replaces the measured document or reads values.
(() => {
  if (window.name === '__vessel_login_render_transport') return;
  const indices = [];
  for (let view = window; view !== view.top; view = view.parent) {
    let index = 0;
    for (; index < view.parent.length; index++) {
      if (view.parent[index] === view) break;
    }
    indices.unshift(index);
  }
  const id = `frame${indices.join('x') || 'main'}`;
  const resourceErrors = [];
  const cspBlocked = [];
  let consentDismissed = false;
  const origin = (value) => {
    try {
      return new URL(value, location.href).origin;
    } catch {
      return 'unknown';
    }
  };
  addEventListener(
    'error',
    (event) => {
      if (event.target?.src && resourceErrors.length < 12) {
        resourceErrors.push(origin(event.target.src));
      }
    },
    true,
  );
  addEventListener('securitypolicyviolation', (event) => {
    if (cspBlocked.length < 12) cspBlocked.push(origin(event.blockedURI));
  });
  const report = () => {
    if (!document.documentElement) return;
    if (window.top === window) {
      const reject = document.querySelector('#onetrust-reject-all-handler');
      if (reject && reject.getBoundingClientRect().width) {
        reject.click();
        consentDismissed = true;
      }
    }
    const fields = [
      ...document.querySelectorAll(
        'input[type=email],input[type=password],input[name=email],input[name=identifier],input[autocomplete=username],input[placeholder*="email" i]',
      ),
    ];
    const visibleLoginFields = fields.filter((node) => {
      const rect = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      let opacity = 1;
      for (let parent = node; parent; parent = parent.parentElement) {
        opacity *= Number(getComputedStyle(parent).opacity);
      }
      const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.bottom > 0 &&
        rect.y < innerHeight &&
        style.visibility !== 'hidden' &&
        style.display !== 'none' &&
        opacity > 0.9 &&
        (hit === node || node.contains(hit))
      );
    }).length;
    const data = {
      id,
      origin: location.origin,
      ready: document.readyState === 'complete',
      visibleLoginFields,
      resourceErrors,
      cspBlocked,
      consentDismissed,
    };
    const transport = document.createElement('iframe');
    transport.name = '__vessel_login_render_transport';
    transport.style.display = 'none';
    transport.src = `vessel-login-render://report?data=${encodeURIComponent(JSON.stringify(data))}`;
    document.documentElement.append(transport);
    setTimeout(() => transport.remove(), 2000);
  };
  setInterval(report, 1000);
  setTimeout(report, 100);
})();
