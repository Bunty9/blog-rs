/* ===========================================================================
   blog-rs — chrome.js
   Injects the shared public header + footer into [data-chrome] pages.
   Set on <body>: data-nav="home|tags|series" and data-base="" or "../".
   =========================================================================== */
(function () {
  var body = document.body;
  if (!body.hasAttribute('data-chrome')) return;
  var base = body.getAttribute('data-base') || '';
  var nav = body.getAttribute('data-nav') || '';

  function ico(p, extra) {
    return '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' + p + '</svg>';
  }
  var searchIcon = ico('<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3"/>');
  var moonIcon = ico('<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>');

  var header =
    '<a class="skip-link" href="#main">Skip to content</a>' +
    '<header class="site-header">' +
      '<div class="container">' +
        '<a class="wordmark" href="' + base + 'index.html"><span class="dot"></span><span>blog-rs</span></a>' +
        '<nav class="nav" aria-label="Primary">' +
          '<a href="' + base + 'public/home.html"' + (nav === 'home' ? ' class="active"' : '') + '>Home</a>' +
          '<a href="' + base + 'public/tags.html"' + (nav === 'tags' ? ' class="active"' : '') + '>Tags</a>' +
          '<a href="' + base + 'public/series.html"' + (nav === 'series' ? ' class="active"' : '') + '>Series</a>' +
          '<a href="' + base + 'public/page.html">About</a>' +
        '</nav>' +
        '<div class="header-actions">' +
          '<a class="icon-btn" href="' + base + 'public/search.html" aria-label="Search">' + searchIcon + '</a>' +
          '<button class="icon-btn" aria-label="Toggle theme" onclick="toggleTheme()">' + moonIcon + '</button>' +
          '<a class="btn btn-outline-accent btn-sm" href="' + base + 'public/members-signup.html">Subscribe</a>' +
          '<button class="icon-btn hamburger" data-toggle="mobile-sheet" aria-label="Menu">' + ico('<path d="M3 6h18M3 12h18M3 18h18"/>') + '</button>' +
        '</div>' +
      '</div>' +
    '</header>' +
    '<div class="mobile-sheet">' +
      '<nav><a href="' + base + 'public/home.html">Home</a><a href="' + base + 'public/tags.html">Tags</a><a href="' + base + 'public/series.html">Series</a><a href="' + base + 'public/page.html">About</a><a href="' + base + 'public/search.html">Search</a></nav>' +
    '</div>';

  var footer =
    '<footer class="site-footer">' +
      '<div class="container">' +
        '<div>' +
          '<div class="wordmark" style="margin-bottom:8px"><span class="dot"></span><span>blog-rs</span></div>' +
          '<p class="tagline">An engineering blog engine written in Rust. Notes on systems, performance, and the craft of building software.</p>' +
        '</div>' +
        '<nav><h4>Explore</h4><a href="' + base + 'public/home.html">Latest</a><a href="' + base + 'public/tags.html">Tags</a><a href="' + base + 'public/series.html">Series</a></nav>' +
        '<nav><h4>More</h4><a href="' + base + 'public/page.html">About</a><a href="#">RSS feed</a><a href="' + base + 'public/members-signup.html">Subscribe</a></nav>' +
        '<div class="foot-bottom">' +
          '<span class="copyright">© 2026 blog-rs · built with rust + askama + htmx</span>' +
          '<button class="icon-btn" aria-label="Toggle theme" onclick="toggleTheme()">' + moonIcon + '</button>' +
        '</div>' +
      '</div>' +
    '</footer>';

  var h = document.querySelector('[data-chrome-header]');
  var f = document.querySelector('[data-chrome-footer]');
  if (h) h.outerHTML = header;
  if (f) f.outerHTML = footer;

  // mobile-sheet styling injected once
  var css = '.mobile-sheet.open{display:block;position:fixed;top:var(--header-h);left:0;right:0;background:var(--bg);border-bottom:1px solid var(--border);z-index:99;padding:var(--space-3)}.mobile-sheet.open nav{display:flex;flex-direction:column;gap:4px}.mobile-sheet.open nav a{padding:12px;text-decoration:none;color:var(--text-2);border-radius:var(--r-md)}.mobile-sheet.open nav a:hover{background:var(--surface)}@media(max-width:768px){.site-header .nav{display:none}.hamburger{display:inline-flex}.header-actions .btn-outline-accent{display:none}}';
  var s = document.createElement('style'); s.textContent = css; document.head.appendChild(s);
})();
