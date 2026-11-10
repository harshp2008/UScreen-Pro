// Landing page behaviour: reveals elements as they scroll into view and switches the screenshot gallery.
// Without JavaScript the page is fully visible (the .js class that hides .reveal is added here).
(function () {
  document.documentElement.classList.add('js');
  var items = document.querySelectorAll('.landing .reveal');
  if ('IntersectionObserver' in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) {
          e.target.classList.add('in');
          io.unobserve(e.target);
        }
      });
    }, { threshold: 0.12 });
    items.forEach(function (el) { io.observe(el); });
  } else {
    items.forEach(function (el) { el.classList.add('in'); });
  }
  // Gallery: each button names the image it shows.
  var img = document.getElementById('gallery-img');
  var buttons = document.querySelectorAll('.landing .thumbs button');
  buttons.forEach(function (b) {
    b.addEventListener('click', function () {
      if (img) { img.src = b.getAttribute('data-src'); img.alt = b.getAttribute('data-alt'); }
      buttons.forEach(function (o) { o.setAttribute('aria-pressed', o === b ? 'true' : 'false'); });
    });
  });
})();
