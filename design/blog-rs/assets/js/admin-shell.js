/* ===========================================================================
   blog-rs — admin-shell.js
   Injects the admin sidebar into [data-admin] pages. Sidebar collapse toggle.
   Set on <body>: data-admin-nav="dashboard|posts|pages|media|members|settings".
   =========================================================================== */
(function () {
  var body = document.body;
  if (!body.hasAttribute('data-admin')) return;
  var active = body.getAttribute('data-admin-nav') || '';

  function ico(p) { return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' + p + '</svg>'; }
  var items = [
    ['dashboard', 'Dashboard', 'dashboard.html', '<rect x="3" y="3" width="7" height="9" rx="1"/><rect x="14" y="3" width="7" height="5" rx="1"/><rect x="14" y="12" width="7" height="9" rx="1"/><rect x="3" y="16" width="7" height="5" rx="1"/>'],
    ['posts', 'Posts', 'posts.html', '<path d="M4 4h16v16H4z"/><path d="M8 8h8M8 12h8M8 16h5"/>'],
    ['pages', 'Pages', 'pages.html', '<path d="M14 3v5h5"/><path d="M17 21H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7l5 5v11a2 2 0 0 1-2 2z"/>'],
    ['media', 'Media', 'media.html', '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="M21 15l-5-5L5 21"/>'],
    ['members', 'Members', 'members.html', '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/>'],
    ['analytics', 'Analytics', 'dashboard.html#analytics', '<path d="M3 3v18h18"/><path d="M7 14l4-4 3 3 5-6"/>'],
    ['settings', 'Settings', 'settings.html', '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>']
  ];

  var nav = items.map(function (it) {
    return '<a href="' + it[2] + '"' + (active === it[0] ? ' class="active"' : '') + '>' +
      ico(it[3]) + '<span class="label">' + it[1] + '</span></a>';
  }).join('');

  var sidebar =
    '<aside class="admin-sidebar">' +
      '<div class="as-brand"><a class="wordmark" href="dashboard.html"><span class="dot"></span><span>blog-rs</span></a></div>' +
      '<nav aria-label="Admin">' + nav + '</nav>' +
      '<div class="as-foot"><a href="../public/home.html" class="" style="display:flex;align-items:center;gap:12px;padding:8px 12px;border-radius:8px;text-decoration:none;color:var(--text-2);font-size:0.9rem">' +
        ico('<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="M16 17l5-5-5-5"/><path d="M21 12H9"/>').replace('<svg', '<svg width="18" height="18"') +
        '<span class="label">Log out</span></a></div>' +
    '</aside>';

  var mount = document.querySelector('[data-admin-sidebar]');
  if (mount) mount.outerHTML = sidebar;

  // collapse toggle
  window.toggleSidebar = function () {
    document.querySelector('.admin').classList.toggle('collapsed');
  };
})();
