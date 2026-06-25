import { request } from "./client";

/** The profile a minted MCP token carries — the same set the management agent
 *  offers, mapped 1:1 onto the backend's capability profiles. */
export type McpTokenProfile = "read_only" | "author" | "author_and_manage";

/** The response of `POST /mcp/token`: the bearer plus the `/mcp` URL an external
 *  client registers against. */
export interface MintMcpTokenResult {
  token: string;
  profile: string;
  mcp_url: string;
}

/** `POST /mcp/token` — mint a profile-scoped management token for an external MCP
 *  client. Requires `Author` (uses the loop token). */
export function mintMcpToken(
  profile: McpTokenProfile,
  label?: string,
): Promise<MintMcpTokenResult> {
  return request<MintMcpTokenResult>("/mcp/token", {
    method: "POST",
    auth: true,
    body: { profile, label: label?.trim() || undefined },
  });
}
