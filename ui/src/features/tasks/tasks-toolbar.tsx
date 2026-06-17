import { ArrowUpFromLine, RefreshCw, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils/cn";

/** The board toolbar: search, promote, refresh. */
export function TasksToolbar({
  query,
  onQuery,
  onPromote,
  onRefresh,
  promoting,
  refreshing,
}: {
  query: string;
  onQuery: (q: string) => void;
  onPromote: () => void;
  onRefresh: () => void;
  promoting: boolean;
  refreshing: boolean;
}) {
  return (
    <div className="flex items-center gap-2">
      <div className="relative w-56">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          placeholder="Filter tasks…"
          className="h-8 pl-8 text-xs"
        />
      </div>
      <Button variant="secondary" size="sm" onClick={onRefresh}>
        <RefreshCw className={cn(refreshing && "animate-spin")} />
        Refresh
      </Button>
      <Button size="sm" onClick={onPromote} disabled={promoting}>
        <ArrowUpFromLine className={cn(promoting && "animate-pulse")} />
        Promote ready
      </Button>
    </div>
  );
}
