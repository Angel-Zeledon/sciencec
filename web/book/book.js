// The Science Book: the mobile chapter drawer, "on this page" tracking,
// and left/right arrow keys for the previous and next chapter.
(function () {
  var menu = document.querySelector(".bar .menu");
  if (menu) menu.addEventListener("click", function () {
    var open = document.body.classList.toggle("open");
    menu.setAttribute("aria-expanded", open ? "true" : "false");
  });

  var here = document.querySelector(".chapters a.here");
  if (here) here.scrollIntoView({ block: "center" });

  var links = Array.prototype.slice.call(document.querySelectorAll(".onpage a"));
  var heads = links.map(function (a) { return document.getElementById(a.getAttribute("href").slice(1)); });
  function track() {
    var current = 0;
    heads.forEach(function (h, i) { if (h && h.getBoundingClientRect().top < 120) current = i; });
    links.forEach(function (a, i) { a.classList.toggle("here", i === current); });
  }
  if (links.length) { document.addEventListener("scroll", track, { passive: true }); track(); }

  document.addEventListener("keydown", function (e) {
    if (e.altKey || e.ctrlKey || e.metaKey || /input|textarea/i.test(e.target.tagName)) return;
    var a = e.key === "ArrowLeft" ? document.querySelector(".pager .prev")
          : e.key === "ArrowRight" ? document.querySelector(".pager .next") : null;
    if (a) location.href = a.href;
  });
})();
