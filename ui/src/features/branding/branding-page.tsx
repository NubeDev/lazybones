import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ImagePlus,
  Palette,
  Plus,
  ServerCrash,
  Trash2,
  X,
} from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogClose,
} from "@/components/ui/dialog";
import { BrandSwatch } from "@/components/brand-picker";
import { ApiError } from "@/lib/api/client";
import { assetUrl, uploadAsset } from "@/lib/api/assets";
import {
  useBranding,
  useBrandingList,
  useCreateBranding,
  useDeleteBranding,
  useUpdateBranding,
} from "@/lib/hooks/use-branding";
import type { BrandingDraft } from "@/lib/api/branding";
import {
  EMPTY_COLORS,
  EMPTY_FONTS,
  type Branding,
  type BrandColors,
} from "@/types/branding";

const EMPTY: BrandingDraft = {
  name: "",
  logo_asset_id: null,
  colors: { ...EMPTY_COLORS },
  fonts: { ...EMPTY_FONTS },
  header_text: "",
  footer_text: "",
};

function draftFrom(b: Branding): BrandingDraft {
  return {
    name: b.name,
    logo_asset_id: b.logo_asset_id ?? null,
    colors: { ...EMPTY_COLORS, ...b.colors },
    fonts: { ...EMPTY_FONTS, ...b.fonts },
    header_text: b.header_text ?? "",
    footer_text: b.footer_text ?? "",
  };
}

/** The standalone Branding home — a catalogue of reusable brand profiles (logo
 *  + colors + fonts + header/footer). Deliberately *not* a Documents subfeature:
 *  any surface references a brand by id (the PDF exporter today, UI theming
 *  later). Selecting a brand (or "New brand") opens a full-page editor. */
export function BrandingPage() {
  const { data: brands, isLoading, error } = useBrandingList();
  // `null` = list; `{ id }` = edit; `{ id: undefined }` = author.
  const [open, setOpen] = useState<{ id?: string } | null>(null);

  if (open) {
    return <BrandEditor brandingId={open.id} onBack={() => setOpen(null)} />;
  }

  const subtitle = brands
    ? `${brands.length} brand profile${brands.length === 1 ? "" : "s"}`
    : "Loading…";

  const newButton = (
    <Button size="sm" onClick={() => setOpen({})}>
      <Plus /> New brand
    </Button>
  );

  return (
    <div className="flex h-full flex-col">
      <Topbar title="Branding" subtitle={subtitle} actions={newButton} />
      <div className="flex-1 overflow-auto p-5">
        {error ? (
          <EmptyState
            icon={ServerCrash}
            title="Can't load brand profiles"
            description={error instanceof ApiError ? error.message : "Unexpected error"}
          />
        ) : isLoading && !brands ? (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <Skeleton key={i} className="h-28 w-full" />
            ))}
          </div>
        ) : !brands || brands.length === 0 ? (
          <EmptyState
            icon={Palette}
            title="No brand profiles yet"
            description="Create a reusable brand (logo, colors, fonts, header/footer) once, then pick it wherever branding is relevant — starting with documents."
            action={newButton}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {brands.map((b) => (
              <BrandCard key={b.id} brand={b} onOpen={() => setOpen({ id: b.id })} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function BrandCard({ brand, onOpen }: { brand: Branding; onOpen: () => void }) {
  return (
    <Card
      className="cursor-pointer p-4 transition-colors hover:border-border-strong"
      onClick={onOpen}
    >
      <div className="flex items-center gap-3">
        {brand.logo_asset_id ? (
          <img
            src={assetUrl(brand.logo_asset_id)}
            alt=""
            className="size-10 shrink-0 rounded-md border border-border object-contain"
          />
        ) : (
          <div className="flex size-10 shrink-0 items-center justify-center rounded-md border border-border bg-surface-2 text-muted-foreground">
            <Palette className="size-4" />
          </div>
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{brand.name}</p>
          <p className="truncate font-mono text-[10px] text-muted-foreground">{brand.id}</p>
        </div>
      </div>
      <div className="mt-3">
        <BrandSwatch brand={brand} />
      </div>
    </Card>
  );
}

const COLOR_FIELDS: { key: keyof BrandColors; label: string }[] = [
  { key: "primary", label: "Primary" },
  { key: "secondary", label: "Secondary" },
  { key: "accent", label: "Accent" },
  { key: "text", label: "Text" },
  { key: "background", label: "Background" },
];

function BrandEditor({
  brandingId,
  onBack,
}: {
  brandingId?: string;
  onBack: () => void;
}) {
  const creating = brandingId == null;
  const { data: brand, isLoading } = useBranding(brandingId);

  const [id, setId] = useState(brandingId ?? "");
  const [draft, setDraft] = useState<BrandingDraft>(EMPTY);

  // Seed the editable draft from the server *once per brand*, not on every
  // refetch. The single-brand query refetches on window focus (react-query
  // default), and the file picker blurs/refocuses the window — re-seeding on
  // that refetch would clobber a just-uploaded logo (and any other unsaved
  // edits) back to the server's values. Guarding by id keeps in-progress edits.
  const seededId = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (brand && seededId.current !== brand.id) {
      seededId.current = brand.id;
      setId(brand.id);
      setDraft(draftFrom(brand));
    }
  }, [brand]);

  const create = useCreateBranding();
  const update = useUpdateBranding();
  const del = useDeleteBranding();
  const mut = creating ? create : update;

  const [uploadingLogo, setUploadingLogo] = useState(false);
  const [logoError, setLogoError] = useState<string | null>(null);

  async function onLogoFile(file: File | undefined) {
    if (!file) return;
    setUploadingLogo(true);
    setLogoError(null);
    try {
      const asset = await uploadAsset(file);
      setDraft((d) => ({ ...d, logo_asset_id: asset.id }));
    } catch (e) {
      setLogoError(e instanceof ApiError ? e.message : "Upload failed.");
    } finally {
      setUploadingLogo(false);
    }
  }

  function save() {
    const trimmed = id.trim();
    if (!trimmed || !draft.name.trim()) return;
    mut.mutate(
      { id: trimmed, draft },
      { onSuccess: () => onBack() },
    );
  }

  const err = mut.error;
  const message =
    err instanceof ApiError
      ? err.status === 409
        ? `A brand "${id.trim()}" already exists.`
        : err.message
      : err
        ? "Something went wrong."
        : null;

  if (!creating && !brand && isLoading) {
    return (
      <div className="flex h-full flex-col">
        <Topbar title="Brand" subtitle="Branding" />
        <div className="space-y-3 p-6">
          <Skeleton className="h-6 w-48" />
          <Skeleton className="h-32 w-full" />
        </div>
      </div>
    );
  }

  const canSave = !!id.trim() && !!draft.name.trim() && !mut.isPending;

  function setColor(key: keyof BrandColors, val: string) {
    setDraft((d) => ({ ...d, colors: { ...d.colors, [key]: val } }));
  }

  return (
    <div className="flex h-full min-w-0 flex-col">
      <Topbar
        title={creating ? "New brand" : (brand?.name ?? id)}
        subtitle={creating ? "Author a reusable brand profile" : brand?.id}
        actions={
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onBack}>
              <ArrowLeft /> Back
            </Button>
            {!creating && brand && (
              <DeleteBrandButton
                brand={brand}
                pending={del.isPending}
                onConfirm={() => del.mutate(brand.id, { onSuccess: onBack })}
              />
            )}
            <Button size="sm" onClick={save} disabled={!canSave}>
              {creating ? "Create brand" : "Save changes"}
            </Button>
          </div>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-2xl space-y-5 p-6">
          <Field
            label="Brand id"
            hint={creating ? "lowercase, unique, e.g. acme-corp" : "the id is fixed once authored"}
          >
            <Input
              value={id}
              autoFocus={creating}
              disabled={!creating}
              onChange={(e) => setId(e.target.value)}
              placeholder="acme-corp"
              className="font-mono"
            />
          </Field>

          <Field label="Name" hint="shown in the brand picker">
            <Input
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              placeholder="Acme Corporation"
            />
          </Field>

          <Field label="Logo" hint="PNG/SVG/JPG — stored once in the asset library">
            <div className="flex items-center gap-3">
              {draft.logo_asset_id ? (
                <img
                  src={assetUrl(draft.logo_asset_id)}
                  alt=""
                  className="size-14 rounded-md border border-border object-contain"
                />
              ) : (
                <div className="flex size-14 items-center justify-center rounded-md border border-dashed border-border-strong text-muted-foreground">
                  <ImagePlus className="size-5" />
                </div>
              )}
              <div className="space-y-1">
                <label>
                  <input
                    type="file"
                    accept="image/*"
                    className="hidden"
                    onChange={(e) => onLogoFile(e.target.files?.[0])}
                  />
                  <Button asChild size="sm" variant="secondary" disabled={uploadingLogo}>
                    <span>
                      <ImagePlus /> {uploadingLogo ? "Uploading…" : "Upload logo"}
                    </span>
                  </Button>
                </label>
                {draft.logo_asset_id && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setDraft({ ...draft, logo_asset_id: null })}
                  >
                    <X /> Remove
                  </Button>
                )}
                {logoError && (
                  <p className="text-[10px] text-status-blocked">{logoError}</p>
                )}
              </div>
            </div>
          </Field>

          <div>
            <span className="text-xs font-medium">Colors</span>
            <div className="mt-1 grid grid-cols-2 gap-3 sm:grid-cols-3">
              {COLOR_FIELDS.map((c) => (
                <label key={c.key} className="space-y-1">
                  <span className="block text-[10px] text-muted-foreground">{c.label}</span>
                  <div className="flex items-center gap-2">
                    <input
                      type="color"
                      value={normalizeColor(draft.colors[c.key])}
                      onChange={(e) => setColor(c.key, e.target.value)}
                      className="size-8 shrink-0 cursor-pointer rounded border border-border bg-surface-2"
                    />
                    <Input
                      value={draft.colors[c.key]}
                      onChange={(e) => setColor(c.key, e.target.value)}
                      placeholder="#1f6feb"
                      className="font-mono text-xs"
                    />
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label="Heading font" hint="e.g. Inter, Georgia">
              <Input
                value={draft.fonts.heading}
                onChange={(e) =>
                  setDraft({ ...draft, fonts: { ...draft.fonts, heading: e.target.value } })
                }
                placeholder="Inter"
              />
            </Field>
            <Field label="Body font">
              <Input
                value={draft.fonts.body}
                onChange={(e) =>
                  setDraft({ ...draft, fonts: { ...draft.fonts, body: e.target.value } })
                }
                placeholder="Inter"
              />
            </Field>
          </div>

          <Field label="Header text" hint="rendered at the top of branded output">
            <Input
              value={draft.header_text}
              onChange={(e) => setDraft({ ...draft, header_text: e.target.value })}
              placeholder="Acme Corporation — Confidential"
            />
          </Field>

          <Field label="Footer text" hint="rendered at the bottom of branded output">
            <Input
              value={draft.footer_text}
              onChange={(e) => setDraft({ ...draft, footer_text: e.target.value })}
              placeholder="© 2026 Acme Corporation"
            />
          </Field>

          {message && (
            <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
              {message}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

/** `<input type=color>` only accepts `#rrggbb`; fall back to black for anything
 *  else (named colors, rgb()) so the swatch still renders without erroring. */
function normalizeColor(value: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(value) ? value : "#000000";
}

function DeleteBrandButton({
  brand,
  pending,
  onConfirm,
}: {
  brand: Branding;
  pending: boolean;
  onConfirm: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" title="Delete brand">
          <Trash2 /> Delete
        </Button>
      </DialogTrigger>
      <DialogContent
        title={`Delete ${brand.id}?`}
        description="This removes the brand profile. Documents referencing it fall back to the default brand."
      >
        <div className="mt-2 flex justify-end gap-2">
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant="destructive"
            size="sm"
            disabled={pending}
            onClick={() => {
              setOpen(false);
              onConfirm();
            }}
          >
            <Trash2 /> Delete
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[10px] text-muted-foreground">{hint}</span>}
    </label>
  );
}
