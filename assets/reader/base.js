// Minimal reader chrome. No framework, no build.
(function () {
    var y = document.querySelector('[data-year]');
    if (y) y.textContent = new Date().getFullYear();
})();
