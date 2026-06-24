/** A deliberately tiny semver range check — just enough for SDK version
 *  negotiation (design §4.3). Supports the comparators a remote realistically
 *  declares for `frontend.sdk_range`: `*`/`x`, exact (`1.2.3`), caret (`^1.2.3`),
 *  tilde (`~1.2.3`), and `||`-separated unions. Anything it cannot parse is
 *  treated as *not satisfied* (conservative — surfaced, not silently mounted). */

type Triple = [number, number, number];

function parse(version: string): Triple | null {
  const core = version.trim().split(/[-+]/)[0]; // drop pre-release / build
  const parts = core.split(".");
  if (parts.length === 0 || parts.length > 3) return null;
  const nums = parts.map((p) => (p === "x" || p === "*" || p === "" ? 0 : Number(p)));
  if (nums.some((n) => !Number.isFinite(n))) return null;
  return [nums[0] ?? 0, nums[1] ?? 0, nums[2] ?? 0];
}

function gte(a: Triple, b: Triple): boolean {
  for (let i = 0; i < 3; i++) {
    if (a[i] > b[i]) return true;
    if (a[i] < b[i]) return false;
  }
  return true;
}

function lt(a: Triple, b: Triple): boolean {
  return !gte(a, b);
}

function satisfiesOne(version: Triple, comparator: string): boolean {
  const c = comparator.trim();
  if (c === "" || c === "*" || c === "x" || c === "latest") return true;

  if (c.startsWith("^")) {
    const base = parse(c.slice(1));
    if (!base) return false;
    // Compatible within the left-most non-zero: ^0.1.x ⇒ >=0.1.0 <0.2.0.
    const upper: Triple =
      base[0] > 0
        ? [base[0] + 1, 0, 0]
        : base[1] > 0
          ? [0, base[1] + 1, 0]
          : [0, 0, base[2] + 1];
    return gte(version, base) && lt(version, upper);
  }

  if (c.startsWith("~")) {
    const base = parse(c.slice(1));
    if (!base) return false;
    const upper: Triple = [base[0], base[1] + 1, 0];
    return gte(version, base) && lt(version, upper);
  }

  if (c.startsWith(">=")) {
    const base = parse(c.slice(2));
    return base ? gte(version, base) : false;
  }

  const exact = parse(c);
  return exact ? version[0] === exact[0] && version[1] === exact[1] && version[2] === exact[2] : false;
}

/** Whether `version` satisfies the (small) `range` grammar above. */
export function satisfies(version: string, range: string): boolean {
  const v = parse(version);
  if (!v) return false;
  return range
    .split("||")
    .some((clause) => clause.trim().split(/\s+/).every((cmp) => satisfiesOne(v, cmp)));
}
