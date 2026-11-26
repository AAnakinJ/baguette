// Root page: go to the visitor's language (the list of links stays for everyone else).
(function () {
  var supported = ["en", "fr", "es", "pt", "de", "ja", "ko", "hi", "ar", "id"];
  var wanted = (navigator.languages || [navigator.language || "en"]).map(function (l) {
    return String(l).slice(0, 2).toLowerCase();
  });
  for (var i = 0; i < wanted.length; i++) {
    if (supported.indexOf(wanted[i]) >= 0) {
      location.replace(wanted[i] + "/");
      return;
    }
  }
  location.replace("en/");
})();
