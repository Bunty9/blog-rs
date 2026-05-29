// Phase 1c placeholder: wire click-to-open on rust playground blocks.
// Real CodeMirror integration follows in a later phase. The shortcode
// asset path is reserved here so the manifest entry resolves to a 200.
(function () {
    function init() {
        document.querySelectorAll('.code-block[data-playground="rust"]').forEach(function (block) {
            block.style.cursor = "pointer";
            block.addEventListener("click", function () {
                var code = block.querySelector("code").innerText;
                var url = "https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&code=" + encodeURIComponent(code);
                window.open(url, "_blank", "noopener");
            });
        });
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init);
    } else {
        init();
    }
})();
