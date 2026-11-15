// Recomputes a receipt's evidence_sha256 in the browser (docs/SPEC.md §6.3).
// It hashes the canonical evidence text the server embedded in the page and
// compares the digest with the hash the page prints. Nothing is sent
// anywhere; results are written with textContent only.
(function () {
  "use strict";

  var source = document.getElementById("canonical-evidence");
  var printed = document.getElementById("evidence-hash");
  var slot = document.getElementById("verify-slot");
  if (!source || !printed || !slot) {
    return;
  }

  var subtle = window.crypto && window.crypto.subtle;
  if (!subtle || typeof TextEncoder === "undefined") {
    slot.textContent =
      "This browser cannot hash here (WebCrypto needs HTTPS or localhost). " +
      "Use tools/recompute_receipt.py instead.";
    return;
  }

  var button = document.createElement("button");
  button.type = "button";
  button.className = "verify-button";
  button.textContent = "Verify hash in this browser";

  var result = document.createElement("p");
  result.className = "verify-result";
  result.setAttribute("role", "status");

  function toHex(buffer) {
    var bytes = new Uint8Array(buffer);
    var hex = "";
    for (var i = 0; i < bytes.length; i += 1) {
      hex += (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16);
    }
    return hex;
  }

  button.addEventListener("click", function () {
    var text = source.textContent;
    var encoded = new TextEncoder().encode(text);
    var expected = printed.textContent.trim().toLowerCase();
    subtle.digest("SHA-256", encoded).then(
      function (digest) {
        var actual = toHex(digest);
        if (actual === expected) {
          result.className = "verify-result verify-ok";
          result.textContent =
            "MATCH: SHA-256 of the " + encoded.length +
            " canonical evidence bytes is " + actual + ".";
        } else {
          result.className = "verify-result verify-bad";
          result.textContent =
            "MISMATCH: the evidence in this page hashes to " + actual +
            ", not the printed " + expected + ". Do not rely on this receipt.";
        }
      },
      function () {
        result.className = "verify-result verify-bad";
        result.textContent = "The browser could not compute the hash.";
      }
    );
  });

  slot.appendChild(button);
  slot.appendChild(result);
})();
