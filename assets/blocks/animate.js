// Apply Motion One animations to every .animate-block with a known preset.
(function () {
    var PRESETS = {
        "fade":       [{ opacity: 0 }, { opacity: 1 }],
        "slide-up":   [{ opacity: 0, transform: "translateY(20px)" }, { opacity: 1, transform: "translateY(0)" }],
        "slide-left": [{ opacity: 0, transform: "translateX(20px)" }, { opacity: 1, transform: "translateX(0)" }],
        "scale":      [{ opacity: 0, transform: "scale(0.95)" }, { opacity: 1, transform: "scale(1)" }],
    };
    function init() {
        var anim = (typeof Motion !== "undefined" && Motion.animate) || null;
        if (!anim) {
            console.warn("[animate] Motion One missing — vendor bundle is a placeholder?");
            return;
        }
        var io = new IntersectionObserver(function (entries) {
            entries.forEach(function (e) {
                if (!e.isIntersecting) return;
                var preset = e.target.getAttribute("data-preset") || "fade";
                var custom = e.target.getAttribute("data-keyframes");
                var kf;
                if (preset === "custom" && custom) {
                    try { kf = JSON.parse(custom); } catch (_) { kf = PRESETS.fade; }
                } else {
                    kf = PRESETS[preset] || PRESETS.fade;
                }
                anim(e.target, kf, { duration: 0.6, easing: "ease-out" });
                io.unobserve(e.target);
            });
        });
        document.querySelectorAll(".animate-block").forEach(function (n) { io.observe(n); });
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();
