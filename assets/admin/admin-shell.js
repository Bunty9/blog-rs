/* ===========================================================================
   blog-rs — admin-shell.js
   Sidebar collapse toggle for server-rendered admin shell.
   The sidebar is rendered server-side (with CSRF logout form); this script
   only wires up the collapse toggle and any other shell behaviours.
   =========================================================================== */
(function () {
  // collapse toggle — called by the hamburger button in the topbar
  window.toggleSidebar = function () {
    var admin = document.querySelector('.admin');
    if (admin) admin.classList.toggle('collapsed');
  };
})();
