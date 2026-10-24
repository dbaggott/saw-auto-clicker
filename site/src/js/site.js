// Everything here happens in the browser. The chosen digit is never sent,
// stored or logged; the "verification" accepts any digit.

const verify = document.querySelector("[data-verify]");
const digit = document.getElementById("digit");
const error = document.querySelector("[data-error]");
const progress = document.querySelector("[data-progress]");
const download = document.querySelector("[data-download]");

const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const steps = [
  (d) => `Verifying digit ${d}…`,
  () => "Cross-referencing with absolutely nobody…",
  (d) => `Verified. Digit ${d} has been securely forgotten.`,
];

document.querySelector("[data-unlock]")?.addEventListener("click", () => {
  if (!digit.value) {
    error.textContent = "Pick a digit first. Any digit. Seriously, any one.";
    digit.focus();
    return;
  }
  const chosen = digit.value;
  digit.value = "";
  error.textContent = "";
  verify.hidden = true;

  const delay = reducedMotion ? 0 : 900;
  steps.forEach((step, i) => {
    setTimeout(() => {
      progress.textContent = step(chosen);
      if (i === steps.length - 1) {
        download.hidden = false;
        download.querySelector("a").focus();
      }
    }, delay * i);
  });
});

for (const button of document.querySelectorAll("[data-sold-out]")) {
  button.addEventListener("click", () => {
    button.hidden = true;
    button.nextElementSibling.hidden = false;
  });
}
