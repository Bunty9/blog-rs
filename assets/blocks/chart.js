// Wire Chart.js to every <canvas data-chart-type> in the page.
// Fetches inline JSON from data-chart-inline, or external JSON from data-chart-src.
(function () {
    function init() {
        if (typeof Chart === "undefined") {
            console.warn("[chart] Chart.js global missing — vendor bundle is a placeholder?");
            return;
        }
        var nodes = document.querySelectorAll("canvas[data-chart-type]");
        nodes.forEach(function (canvas) {
            var type = canvas.getAttribute("data-chart-type");
            var inline = canvas.getAttribute("data-chart-inline");
            var src = canvas.getAttribute("data-chart-src");
            function draw(payload) {
                try {
                    new Chart(canvas, { type: type, data: payload.data || payload, options: payload.options || {} });
                } catch (e) {
                    console.error("[chart] init failed", e);
                }
            }
            if (inline) {
                try { draw(JSON.parse(inline)); } catch (e) { console.error("[chart] bad inline JSON", e); }
            } else if (src) {
                fetch(src).then(function (r) { return r.json(); }).then(draw).catch(function (e) {
                    console.error("[chart] fetch failed", src, e);
                });
            }
        });
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();
