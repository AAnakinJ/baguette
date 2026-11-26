// Prices for the visitor's country and lifetime seats left (from the licence server).
// Without JavaScript or offline, the page shows the base prices.
(function () {
  var body = document.body;
  var api = body.getAttribute("data-api");
  var lang = body.getAttribute("data-lang") || "en";
  if (!api || !window.fetch) return;
  var fmt = new Intl.NumberFormat(lang === "pt" ? "pt-BR" : lang, { style: "currency", currency: "EUR", minimumFractionDigits: 0, maximumFractionDigits: 2 });
  fetch(api + "/v1/prices")
    .then(function (r) {
      return r.ok ? r.json() : null;
    })
    .then(function (p) {
      if (!p) return;
      document.querySelectorAll("[data-price]").forEach(function (el) {
        var v = p.prices[el.getAttribute("data-price")];
        if (typeof v === "number") el.textContent = fmt.format(v);
      });
      var left = document.querySelector("[data-lifetime-left]");
      if (left && typeof p.lifetime_left === "number") {
        left.textContent = p.lifetime_left > 0 ? left.getAttribute("data-template").replace("{n}", p.lifetime_left) : left.getAttribute("data-sold-out");
        if (p.lifetime_left <= 0) {
          var btn = left.parentNode.querySelector(".btn");
          if (btn) btn.setAttribute("aria-disabled", "true");
        }
      }
    })
    .catch(function () {});
})();
