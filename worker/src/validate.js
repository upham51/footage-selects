// Checking a waitlist signup. The database checks the same rules again.

export const EDITORS = {
  resolve: "DaVinci Resolve",
  premiere: "Premiere Pro",
  final_cut: "Final Cut Pro",
  other: "Another editor",
  none: "I don't edit yet",
};

export const SHOOTS = {
  travel: "Travel",
  drone: "Drone",
  real_estate: "Real estate",
  events: "Events",
  weddings: "Weddings",
  outdoor: "Outdoor and adventure",
  other: "Something else",
};

export const HOURS = {
  under_2: "Under 2 hours",
  "2_to_5": "2 to 5 hours",
  "5_to_15": "5 to 15 hours",
  "15_to_40": "15 to 40 hours",
  over_40: "More than 40 hours",
};

const EMAIL = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

/** Returns { ok: true, value } or { ok: false, field, message }. */
export function validateSignup(input) {
  const get = (k) => (typeof input?.[k] === "string" ? input[k].trim() : "");
  const email = get("email");
  if (!email) return { ok: false, field: "email", message: "Enter your email address." };
  if (email.length > 254 || !EMAIL.test(email)) {
    return { ok: false, field: "email", message: "That email address does not look complete. Check for a missing @ or ending such as .com." };
  }
  const editor = get("editor");
  if (!(editor in EDITORS)) return { ok: false, field: "editor", message: "Choose the editor you use, or that you don't edit yet." };

  let shoots = input?.shoots ?? [];
  if (typeof shoots === "string") shoots = [shoots];
  if (!Array.isArray(shoots)) shoots = [];
  shoots = [...new Set(shoots.map((s) => String(s).trim()).filter(Boolean))];
  if (shoots.length === 0) return { ok: false, field: "shoots", message: "Choose at least one kind of footage you shoot." };
  if (shoots.some((s) => !(s in SHOOTS))) return { ok: false, field: "shoots", message: "One of the footage choices was not recognised. Reload the page and try again." };

  let shootsOther = get("shoots_other");
  if (!shoots.includes("other")) shootsOther = "";
  if (shootsOther.length > 100) return { ok: false, field: "shoots_other", message: "Keep the description under 100 characters." };

  const hours = get("hours_per_month");
  if (!(hours in HOURS)) return { ok: false, field: "hours_per_month", message: "Choose roughly how many hours of footage you shoot a month." };

  return { ok: true, value: { email, editor, shoots, shoots_other: shootsOther || null, hours_per_month: hours } };
}
