const search = document.querySelector("[data-server-search]");
if (search) {
  search.addEventListener("input", () => {
    const needle = search.value.trim().toLowerCase();
    document.querySelectorAll("[data-server]").forEach((card) => {
      const name = card.getAttribute("data-server") || "";
      card.hidden = needle.length > 0 && !name.includes(needle);
    });
  });
}

document.querySelectorAll("time[datetime]").forEach((node) => {
  const date = new Date(node.dateTime);
  if (Number.isNaN(date.getTime())) return;
  node.textContent = date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
});

const form = document.querySelector("form[data-save]");
const bar = document.querySelector("[data-savebar]");
if (form && bar) {
  const snapshot = new URLSearchParams(new FormData(form)).toString();
  const age = form.querySelector("[data-age]");
  const ageOut = form.querySelector("[data-age-out]");
  const syncAge = () => {
    if (age && ageOut) ageOut.textContent = `${age.value} days`;
  };
  const check = () => {
    syncAge();
    const now = new URLSearchParams(new FormData(form)).toString();
    bar.hidden = now === snapshot;
  };
  form.addEventListener("input", check);
  form.addEventListener("change", check);
  const discard = form.querySelector("[data-discard]");
  if (discard) {
    discard.addEventListener("click", () => {
      form.reset();
      check();
    });
  }
  syncAge();
}
