import { useMutation } from "@tanstack/react-query";
import { mintMcpToken, type McpTokenProfile } from "@/lib/api/mcp";

/** Mint a profile-scoped MCP token for an external client (`POST /mcp/token`).
 *  Not cached — each call hands out a fresh bearer the operator copies once. */
export function useMintMcpToken() {
  return useMutation({
    mutationFn: ({ profile, label }: { profile: McpTokenProfile; label?: string }) =>
      mintMcpToken(profile, label),
  });
}
