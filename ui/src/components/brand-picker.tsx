import { Palette } from "lucide-react";
import { useBrandingList } from "@/lib/hooks/use-branding";
import type { Branding } from "@/types/branding";

/** A reusable "select which branding to use" control.
 *
 *  Branding is a standalone, app-wide resource (logo + colors + fonts +
 *  header/footer); any feature can drop this in to reference a brand by id. The
 *  document editor is its first consumer, but it is intentionally generic — give
 *  it the current `value` (a `branding_id` or `null`) and an `onChange`.
 *
 *  Empty value ⇒ "default brand" (the server falls back to the seeded default at
 *  render time). A small swatch previews the selected brand's palette. */
export function BrandPicker({
  value,
  onChange,
  allowNone = true,
  className,
}: {
  value: string | null;
  onChange: (brandingId: string | null) => void;
  allowNone?: boolean;
  className?: string;
}) {
  const { data: brands, isLoading } = useBrandingList();
  const selected = brands?.find((b) => b.id === value) ?? null;

  return (
    <div className={className}>
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Palette className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <select
            value={value ?? ""}
            disabled={isLoading}
            onChange={(e) => onChange(e.target.value || null)}
            className="h-9 w-full rounded-md border border-border bg-surface-2 pl-7 pr-2 text-sm outline-none transition-colors focus-visible:border-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            {allowNone && (
              <option value="">{isLoading ? "Loading…" : "Default brand"}</option>
            )}
            {brands?.map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        </div>
        {selected && <BrandSwatch brand={selected} />}
      </div>
    </div>
  );
}

/** A compact row of the brand's five palette colors. */
export function BrandSwatch({ brand }: { brand: Branding }) {
  const colors = [
    brand.colors.primary,
    brand.colors.secondary,
    brand.colors.accent,
    brand.colors.text,
    brand.colors.background,
  ].filter(Boolean);
  if (colors.length === 0) return null;
  return (
    <div className="flex shrink-0 items-center gap-0.5">
      {colors.map((c, i) => (
        <span
          key={i}
          className="size-4 rounded-sm border border-border"
          style={{ backgroundColor: c }}
          title={c}
        />
      ))}
    </div>
  );
}
