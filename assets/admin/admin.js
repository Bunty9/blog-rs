// Admin glue. Loaded after htmx + alpine.
// 1. Auto-include CSRF token on every htmx mutating request.
document.body.addEventListener('htmx:configRequest', (evt) => {
    const meta = document.querySelector('meta[name="csrf-token"]');
    if (meta) {
        evt.detail.headers['X-CSRF-Token'] = meta.getAttribute('content');
    }
});

// 2. Mark sidebar links active based on path prefix.
(() => {
    const path = window.location.pathname;
    document.querySelectorAll('.admin-sidebar nav a').forEach((a) => {
        const href = a.getAttribute('href');
        if (href === '/admin' && path === '/admin') a.classList.add('active');
        else if (href !== '/admin' && path.startsWith(href)) a.classList.add('active');
    });
})();

// 3. Flash auto-dismiss after 4s.
document.body.addEventListener('htmx:afterSettle', () => {
    document.querySelectorAll('.flash[data-autohide]').forEach((el) => {
        setTimeout(() => el.remove(), 4000);
    });
});
