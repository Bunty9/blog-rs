// Admin glue. Loaded after htmx + alpine.
// 1. Auto-include CSRF token on every htmx mutating request.
document.body.addEventListener('htmx:configRequest', (evt) => {
    const meta = document.querySelector('meta[name="csrf-token"]');
    if (meta) {
        evt.detail.headers['X-CSRF-Token'] = meta.getAttribute('content');
    }
});

// 2. Flash auto-dismiss after 4s.
document.body.addEventListener('htmx:afterSettle', () => {
    document.querySelectorAll('.flash[data-autohide]').forEach((el) => {
        setTimeout(() => el.remove(), 4000);
    });
});

// 3. Show htmx error responses in the triggering form's [data-error] slot
//    (htmx doesn't swap 4xx/5xx bodies, so failures were otherwise silent).
document.body.addEventListener('htmx:beforeRequest', (evt) => {
    const slot = evt.detail.elt.closest('form')?.querySelector('[data-error]');
    if (slot) slot.textContent = '';
});
document.body.addEventListener('htmx:responseError', (evt) => {
    const slot = evt.detail.elt.closest('form')?.querySelector('[data-error]');
    if (!slot) return;
    const xhr = evt.detail.xhr;
    let msg = xhr.status === 413 ? 'File too large (max 10 MB)' : `Request failed (${xhr.status})`;
    try { msg = JSON.parse(xhr.responseText).error || msg; } catch (_) { /* not JSON */ }
    slot.textContent = msg;
});
