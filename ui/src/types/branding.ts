/** Mirror of `lazybones_store::BrandColors` — the brand palette (CSS color
 *  strings: `#rrggbb`, `rgb(...)`, a named color, …). */
export interface BrandColors {
  primary: string;
  secondary: string;
  accent: string;
  text: string;
  background: string;
}

/** Mirror of `lazybones_store::BrandFonts` — the brand typography. */
export interface BrandFonts {
  heading: string;
  body: string;
}

/** Mirror of `lazybones_store::Branding` — a standalone, reusable brand profile
 *  (logo + colors + fonts + header/footer) that any feature references by id. */
export interface Branding {
  id: string;
  project?: string | null;
  name: string;
  logo_asset_id?: string | null;
  colors: BrandColors;
  fonts: BrandFonts;
  header_text: string;
  footer_text: string;
  created_at: string;
  updated_at: string;
}

export const EMPTY_COLORS: BrandColors = {
  primary: "#1f6feb",
  secondary: "#6e7781",
  accent: "#0969da",
  text: "#1f2328",
  background: "#ffffff",
};

export const EMPTY_FONTS: BrandFonts = { heading: "", body: "" };
