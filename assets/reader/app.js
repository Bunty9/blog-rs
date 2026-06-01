/* ===========================================================================
   blog-rs — app.js  (tiny progressive-enhancement layer, no framework)
   Theme toggle, mobile sheet, toasts, modal/drawer, copy, tabs/segments.
   =========================================================================== */
(function () {
  'use strict';

  /* ---- Theme: prefers-color-scheme default + persisted manual toggle ---- */
  var KEY = 'blogrs-theme';
  function systemTheme() {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  function applyTheme(t) {
    if (t === 'system' || !t) {
      document.documentElement.removeAttribute('data-theme');
      document.documentElement.setAttribute('data-theme', systemTheme());
    } else {
      document.documentElement.setAttribute('data-theme', t);
    }
    document.querySelectorAll('[data-theme-state]').forEach(function (el) {
      el.setAttribute('data-theme-state', document.documentElement.getAttribute('data-theme'));
    });
  }
  function initTheme() {
    var saved = localStorage.getItem(KEY);
    applyTheme(saved || systemTheme());
  }
  window.toggleTheme = function () {
    var cur = document.documentElement.getAttribute('data-theme') || systemTheme();
    var next = cur === 'dark' ? 'light' : 'dark';
    localStorage.setItem(KEY, next);
    applyTheme(next);
  };
  // run ASAP (the inline head snippet sets it pre-paint; this re-syncs)
  initTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function () {
    if (!localStorage.getItem(KEY)) applyTheme(systemTheme());
  });

  document.addEventListener('DOMContentLoaded', function () {
    /* ---- Mobile nav sheet ---- */
    var sheet = document.querySelector('.mobile-sheet');
    document.querySelectorAll('[data-toggle="mobile-sheet"]').forEach(function (b) {
      b.addEventListener('click', function () {
        if (sheet) sheet.classList.toggle('open');
      });
    });

    /* ---- Tabs ---- */
    document.querySelectorAll('[data-tabs]').forEach(function (group) {
      var btns = group.querySelectorAll('[data-tab]');
      btns.forEach(function (btn) {
        btn.addEventListener('click', function () {
          var name = btn.getAttribute('data-tab');
          btns.forEach(function (b) { b.classList.toggle('active', b === btn); });
          var scope = group.getAttribute('data-tabs');
          document.querySelectorAll('[data-tabpanel="' + scope + '"]').forEach(function (p) {
            p.classList.toggle('hide', p.getAttribute('data-tabname') !== name);
          });
        });
      });
    });

    /* ---- Segmented controls (window selectors etc.) ---- */
    document.querySelectorAll('[data-segment]').forEach(function (seg) {
      seg.querySelectorAll('button').forEach(function (btn) {
        btn.addEventListener('click', function () {
          seg.querySelectorAll('button').forEach(function (b) { b.classList.toggle('active', b === btn); });
          seg.dispatchEvent(new CustomEvent('segment:change', { detail: btn.dataset.value }));
        });
      });
    });

    /* ---- Copy buttons ---- */
    document.querySelectorAll('[data-copy]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var sel = btn.getAttribute('data-copy');
        var src = sel ? document.querySelector(sel) : btn.closest('.code-card').querySelector('pre');
        var text = btn.dataset.copyText || (src ? src.innerText : '');
        navigator.clipboard && navigator.clipboard.writeText(text);
        var label = btn.querySelector('.cc-label') || btn;
        var old = label.textContent;
        label.textContent = 'Copied';
        setTimeout(function () { label.textContent = old; }, 1400);
      });
    });

    /* ---- Modal / drawer open/close ---- */
    document.querySelectorAll('[data-open]').forEach(function (t) {
      t.addEventListener('click', function () {
        var el = document.getElementById(t.getAttribute('data-open'));
        if (el) el.classList.add('open');
      });
    });
    document.querySelectorAll('[data-close]').forEach(function (t) {
      t.addEventListener('click', function () {
        var el = t.closest('.overlay, .drawer');
        if (el) el.classList.remove('open');
      });
    });
    document.querySelectorAll('.overlay').forEach(function (ov) {
      ov.addEventListener('click', function (e) { if (e.target === ov) ov.classList.remove('open'); });
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') {
        document.querySelectorAll('.overlay.open, .drawer.open, .mobile-sheet.open').forEach(function (el) { el.classList.remove('open'); });
      }
    });

    /* ---- Button loading demo ---- */
    document.querySelectorAll('[data-loading-demo]').forEach(function (b) {
      b.addEventListener('click', function () {
        if (b.classList.contains('is-loading')) return;
        var html = b.innerHTML;
        b.classList.add('is-loading');
        b.innerHTML = '<span class="spinner"></span> Saving…';
        setTimeout(function () { b.classList.remove('is-loading'); b.innerHTML = html; window.toast && window.toast('Saved', 'success'); }, 1300);
      });
    });
  });

  /* ---- Toast helper ---- */
  window.toast = function (msg, type) {
    var wrap = document.querySelector('.toast-wrap');
    if (!wrap) { wrap = document.createElement('div'); wrap.className = 'toast-wrap'; wrap.setAttribute('aria-live', 'polite'); document.body.appendChild(wrap); }
    var t = document.createElement('div');
    t.className = 'toast ' + (type || 'success');
    var icon = type === 'error'
      ? '<svg class="t-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M15 9l-6 6M9 9l6 6"/></svg>'
      : '<svg class="t-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 6L9 17l-5-5"/></svg>';
    t.innerHTML = icon + '<span>' + msg + '</span>';
    wrap.appendChild(t);
    setTimeout(function () { t.style.opacity = '0'; t.style.transition = 'opacity .3s'; setTimeout(function () { t.remove(); }, 300); }, 2600);
  };
})();
