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
