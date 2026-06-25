import { useState } from "react";
import { Check, Copy, KeyRound, Plug2 } from "lucide-react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { ApiError } from "@/lib/api/client";
import { useMintMcpToken } from "@/lib/hooks/use-mcp-token";
import type { McpTokenProfile, MintMcpTokenResult } from "@/lib/api/mcp";

/** The "MCP access" settings card, shown next to the Lazybones-Agent card: mint a
 *  profile-scoped bearer token for an external MCP client (Claude Desktop, the
 *  `claude` CLI, Cursor, a custom rmcp client) to drive lazybones through the
 *  in-process `/mcp` endpoint (`docs/mcp/README.md` §9 OQ1).
 *
 *  The token is a strict subset of the loop grant — it never carries
 *  `Claim`/`Secret`/`Extension`, so an external agent authors but cannot start,
 *  install, or read secrets. Minting itself requires the loop (`Author`) token,
 *  which the request boundary attaches automatically. */
export function McpTokenCard() {
  const mint = useMintMcpToken();
  const [profile, setProfile] = useState<McpTokenProfile>("author");
  const [label, setLabel] = useState("");
  const [result, setResult] = useState<MintMcpTokenResult | null>(null);

  const onMint = () => {
    setResult(null);
    mint.mutate(
      { profile, label },
      { onSuccess: (res) => setResult(res) },
    );
  };

  const mintErr = mint.error instanceof ApiError ? mint.error.message : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Plug2 className="size-4 text-accent" /> MCP access
        </CardTitle>
        <CardDescription>
          Mint a bearer token for an external MCP client (Claude Desktop, the{" "}
          <span className="font-mono">claude</span> CLI, Cursor) to drive lazybones
          through the <span className="font-mono">/mcp</span> endpoint. The token is
          a strict subset of the loop grant — it authors only, and never starts,
          installs, or reads secrets.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Field
          label="Profile"
          hint="What the minted token may do. Authoring never includes start/stop/retry/delete."
        >
          <Select
            value={profile}
            onChange={(v) => setProfile(v as McpTokenProfile)}
            options={[
              { value: "author", label: "Author — create/edit + read (default)" },
              {
                value: "author_and_manage",
                label: "Author & manage — + propose lifecycle",
              },
              { value: "read_only", label: "Read only — supervision/read tools" },
            ]}
          />
        </Field>

        <Field
          label="Label"
          hint="An optional tag for the client this token is for (recorded for auditing)."
        >
          <Input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. claude-desktop"
          />
        </Field>

        {result && <TokenReadout result={result} />}

        <Separator />
        <div className="flex items-center justify-end gap-3">
          {mintErr && <span className="text-xs text-status-blocked">{mintErr}</span>}
          <Button onClick={onMint} size="sm" disabled={mint.isPending}>
            <KeyRound className="size-3.5" />
            {mint.isPending ? "Minting…" : "Mint token"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/** The freshly-minted bearer + its `/mcp` URL, with copy-to-clipboard. Shown once
 *  per mint — the operator copies the token into the client's config; it is not
 *  re-fetchable (mint again for a new one). */
function TokenReadout({ result }: { result: MintMcpTokenResult }) {
  return (
    <div className="space-y-2 rounded-md border border-border bg-surface p-3">
      <p className="text-[11px] text-muted-foreground">
        Copy this token into your client's <span className="font-mono">Authorization</span>{" "}
        header now — it isn't shown again.
      </p>
      <CopyRow label="Token" value={result.token} secret />
      <CopyRow label="MCP URL" value={result.mcp_url} />
      <p className="text-[11px] text-muted-foreground">
        Granted profile: <span className="font-mono">{result.profile}</span>
      </p>
    </div>
  );
}

/** A monospace value with a copy button; `secret` masks it until copied. */
function CopyRow({
  label,
  value,
  secret = false,
}: {
  label: string;
  value: string;
  secret?: boolean;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable (insecure context) — the value is selectable below. */
    }
  };

  return (
    <div className="flex items-center gap-2">
      <span className="w-16 shrink-0 text-[11px] font-medium text-muted-foreground">
        {label}
      </span>
      <code className="flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-[11px]">
        {secret ? value.replace(/.(?=.{4})/g, "•") : value}
      </code>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={copy}
        aria-label={`Copy ${label}`}
      >
        {copied ? <Check className="size-3.5 text-status-done" /> : <Copy className="size-3.5" />}
      </Button>
    </div>
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
    <label className="block space-y-1.5">
      <span className="text-xs font-medium">{label}</span>
      {children}
      {hint && <span className="block text-[11px] text-muted-foreground">{hint}</span>}
    </label>
  );
}

/** A minimal styled native select, matching the settings page's plain-HTML idiom. */
function Select({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="h-9 w-full rounded-md border border-border bg-surface px-2.5 text-sm outline-none focus:border-border-strong"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}
