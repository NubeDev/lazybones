import { useEffect, useMemo, useState } from "react";
import { Plus, ServerCrash, Trash2, UserPlus, Users } from "lucide-react";
import { Topbar } from "@/components/layout/topbar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ApiError } from "@/lib/api/client";
import {
  useTeams,
  useMembers,
  useCreateTeam,
  useAddMember,
  useRemoveMember,
} from "@/lib/hooks/use-teams";
import { listMembers } from "@/lib/api/teams";
import { useQueries } from "@tanstack/react-query";
import type { MemberRole } from "@/types/project";

/** Admin — the global, admin-only console: manage teams, membership and per-team
 *  roles. Every mutation here requires `Author` + admin on the backend
 *  (members.rs / teams.rs); the UI only routes admins (or local operators) in. */
export function AdminPage() {
  return (
    <div className="flex h-full flex-col">
      <Topbar title="Admin" subtitle="Teams, users, membership & roles" />
      <div className="flex-1 overflow-auto p-5">
        <Tabs defaultValue="teams">
          <TabsList>
            <TabsTrigger value="teams">Teams</TabsTrigger>
            <TabsTrigger value="membership">Membership & roles</TabsTrigger>
            <TabsTrigger value="users">Users</TabsTrigger>
          </TabsList>
          <TabsContent value="teams">
            <TeamsAdmin />
          </TabsContent>
          <TabsContent value="membership">
            <MembershipAdmin />
          </TabsContent>
          <TabsContent value="users">
            <UsersAdmin />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

/* ── Teams ─────────────────────────────────────────────────────────────── */

function TeamsAdmin() {
  const { data: teams, isLoading, error } = useTeams();

  return (
    <div className="space-y-4 pt-4">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">
          {teams ? `${teams.length} team${teams.length === 1 ? "" : "s"}` : "Loading…"}
        </p>
        <CreateTeamDialog />
      </div>
      {error ? (
        <EmptyState
          icon={ServerCrash}
          title="Can't load teams"
          description={error instanceof ApiError ? error.message : "Unexpected error"}
        />
      ) : isLoading && !teams ? (
        <Skeleton className="h-24 w-full" />
      ) : !teams || teams.length === 0 ? (
        <EmptyState
          icon={Users}
          title="No teams yet"
          description="Create a team — it becomes the owner of projects and the carrier of membership."
          action={<CreateTeamDialog />}
        />
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {teams.map((t) => (
            <Card key={t.id} className="p-4">
              <p className="truncate text-sm font-medium">{t.title}</p>
              <p className="truncate font-mono text-[10px] text-muted-foreground">{t.id}</p>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

function CreateTeamDialog() {
  const [open, setOpen] = useState(false);
  const [id, setId] = useState("");
  const [title, setTitle] = useState("");
  const create = useCreateTeam();

  function submit() {
    const tid = id.trim();
    if (!tid || !title.trim()) return;
    create.mutate(
      { id: tid, title: title.trim() },
      {
        onSuccess: () => {
          setOpen(false);
          setId("");
          setTitle("");
          create.reset();
        },
      },
    );
  }

  const message = mutationMessage(create.error, `A team "${id.trim()}" already exists.`);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus /> New team
        </Button>
      </DialogTrigger>
      <DialogContent title="New team" description="An owner of projects and membership.">
        <div className="space-y-4">
          <label className="block space-y-1">
            <span className="text-xs font-medium">Team id</span>
            <Input
              value={id}
              autoFocus
              onChange={(e) => setId(e.target.value)}
              placeholder="platform"
              className="font-mono"
            />
          </label>
          <label className="block space-y-1">
            <span className="text-xs font-medium">Title</span>
            <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Platform" />
          </label>
          {message && <ErrorBox>{message}</ErrorBox>}
          <div className="flex justify-end gap-2">
            <DialogClose asChild>
              <Button variant="ghost" size="sm">
                Cancel
              </Button>
            </DialogClose>
            <Button
              size="sm"
              onClick={submit}
              disabled={!id.trim() || !title.trim() || create.isPending}
            >
              Create team
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/* ── Membership & roles ────────────────────────────────────────────────── */

function MembershipAdmin() {
  const { data: teams } = useTeams();
  const [teamId, setTeamId] = useState<string | null>(null);

  useEffect(() => {
    if (!teamId && teams && teams.length > 0) setTeamId(teams[0].id);
  }, [teams, teamId]);

  if (!teams || teams.length === 0) {
    return (
      <div className="pt-4">
        <EmptyState
          icon={Users}
          title="No teams yet"
          description="Create a team first, then manage its membership here."
        />
      </div>
    );
  }

  return (
    <div className="space-y-4 pt-4">
      <div className="flex flex-wrap gap-2">
        {teams.map((t) => (
          <button
            key={t.id}
            onClick={() => setTeamId(t.id)}
            className={
              "rounded-md border px-3 py-1.5 text-xs font-medium transition-colors " +
              (teamId === t.id
                ? "border-accent/40 bg-accent-soft/50 text-accent"
                : "border-border bg-surface-2 text-muted-foreground hover:text-foreground")
            }
          >
            {t.title}
          </button>
        ))}
      </div>
      {teamId && <TeamMembers team={teamId} />}
    </div>
  );
}

function TeamMembers({ team }: { team: string }) {
  const { data: members, isLoading } = useMembers(team);
  const add = useAddMember();
  const remove = useRemoveMember();

  const [user, setUser] = useState("");
  const [role, setRole] = useState<MemberRole>("member");

  function submit() {
    const u = user.trim();
    if (!u) return;
    add.mutate(
      { team, user: u, role },
      {
        onSuccess: () => {
          setUser("");
          setRole("member");
          add.reset();
        },
      },
    );
  }

  const message = mutationMessage(add.error, null);

  return (
    <div className="space-y-4">
      <Card className="space-y-3 p-4">
        <p className="text-xs font-medium">Add member</p>
        <div className="flex flex-wrap items-end gap-2">
          <label className="flex-1 space-y-1">
            <span className="block text-[10px] text-muted-foreground">User id</span>
            <Input
              value={user}
              onChange={(e) => setUser(e.target.value)}
              placeholder="ada"
              className="font-mono"
            />
          </label>
          <label className="space-y-1">
            <span className="block text-[10px] text-muted-foreground">Role</span>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as MemberRole)}
              className="h-9 rounded-md border border-border bg-surface-2 px-2 text-sm"
            >
              <option value="member">member</option>
              <option value="manager">manager</option>
            </select>
          </label>
          <Button size="sm" onClick={submit} disabled={!user.trim() || add.isPending}>
            <UserPlus /> Add
          </Button>
        </div>
        {message && <ErrorBox>{message}</ErrorBox>}
      </Card>

      {isLoading && !members ? (
        <Skeleton className="h-20 w-full" />
      ) : !members || members.length === 0 ? (
        <p className="text-xs text-muted-foreground">No members on this team yet.</p>
      ) : (
        <ul className="space-y-1.5">
          {members.map((m) => (
            <li
              key={m.user}
              className="flex items-center justify-between gap-2 rounded-md border border-border bg-surface-2 px-3 py-2"
            >
              <div className="flex items-center gap-2">
                <span className="font-mono text-xs">{m.user}</span>
                <Badge variant={m.role === "manager" ? "accent" : "outline"}>{m.role}</Badge>
              </div>
              <Button
                variant="ghost"
                size="sm"
                disabled={remove.isPending}
                onClick={() => remove.mutate({ team, user: m.user })}
                title="Remove member"
              >
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ── Users (derived from membership across teams) ──────────────────────── */

function UsersAdmin() {
  const { data: teams } = useTeams();
  const teamIds = useMemo(() => (teams ?? []).map((t) => t.id), [teams]);

  // Fan out a members read per team, then fold into a per-user roster of the
  // teams (and roles) they hold. There is no standalone users endpoint — the
  // user set is exactly who is a member somewhere.
  const results = useQueries({
    queries: teamIds.map((id) => ({
      queryKey: ["members", id],
      queryFn: ({ signal }: { signal: AbortSignal }) => listMembers(id, signal),
    })),
  });

  const loading = results.some((r) => r.isLoading);
  const roster = useMemo(() => {
    const byUser = new Map<string, { team: string; role: MemberRole }[]>();
    results.forEach((r, i) => {
      const team = teamIds[i];
      (r.data ?? []).forEach((m) => {
        const list = byUser.get(m.user) ?? [];
        list.push({ team, role: m.role });
        byUser.set(m.user, list);
      });
    });
    return [...byUser.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [results, teamIds]);

  if (!teams || teams.length === 0) {
    return (
      <div className="pt-4">
        <EmptyState
          icon={Users}
          title="No users yet"
          description="Users appear here once they're added to a team in Membership & roles."
        />
      </div>
    );
  }

  return (
    <div className="space-y-3 pt-4">
      <p className="text-xs text-muted-foreground">
        Users are derived from team membership. Add or remove them under Membership & roles.
      </p>
      {loading && roster.length === 0 ? (
        <Skeleton className="h-24 w-full" />
      ) : roster.length === 0 ? (
        <p className="text-xs text-muted-foreground">No users on any team yet.</p>
      ) : (
        <ul className="space-y-1.5">
          {roster.map(([user, memberships]) => (
            <li
              key={user}
              className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-surface-2 px-3 py-2"
            >
              <span className="font-mono text-xs font-medium">{user}</span>
              {memberships.map((m) => (
                <Badge key={m.team} variant={m.role === "manager" ? "accent" : "outline"}>
                  {m.team} · {m.role}
                </Badge>
              ))}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ── helpers ───────────────────────────────────────────────────────────── */

function mutationMessage(err: unknown, conflict: string | null): string | null {
  if (err instanceof ApiError) {
    if (err.status === 409 && conflict) return conflict;
    if (err.status === 403) return "Admin authority is required for this action.";
    if (err.status === 404) return "That team no longer exists.";
    return err.message;
  }
  return err ? "Something went wrong." : null;
}

function ErrorBox({ children }: { children: React.ReactNode }) {
  return (
    <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
      {children}
    </p>
  );
}
