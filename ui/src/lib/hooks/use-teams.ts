import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addMember,
  createTeam,
  listMembers,
  listTeamProjects,
  listTeams,
  removeMember,
} from "@/lib/api/teams";
import type { MemberRole } from "@/types/project";

/** Poll every team (open read). */
export function useTeams() {
  return useQuery({
    queryKey: ["teams"],
    queryFn: ({ signal }) => listTeams(signal),
    refetchInterval: 10000,
  });
}

/** The projects placed `under` one team. Disabled when `team` is absent. */
export function useTeamProjects(team?: string) {
  return useQuery({
    queryKey: ["team-projects", team],
    queryFn: ({ signal }) => listTeamProjects(team as string, signal),
    enabled: team != null,
    refetchInterval: 10000,
  });
}

/** One team's members with their per-team roles. Disabled when `team` is absent. */
export function useMembers(team?: string) {
  return useQuery({
    queryKey: ["members", team],
    queryFn: ({ signal }) => listMembers(team as string, signal),
    enabled: team != null,
  });
}

/** Create (or re-affirm) a team (`POST /teams`). */
export function useCreateTeam() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, title }: { id: string; title: string }) => createTeam(id, title),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["teams"] }),
  });
}

/** Add (or re-affirm) a team member with a role (`POST /teams/:id/members`). */
export function useAddMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ team, user, role }: { team: string; user: string; role: MemberRole }) =>
      addMember(team, user, role),
    onSuccess: (_data, { team }) => qc.invalidateQueries({ queryKey: ["members", team] }),
  });
}

/** Remove a membership (`DELETE /teams/:id/members/:user`). */
export function useRemoveMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ team, user }: { team: string; user: string }) => removeMember(team, user),
    onSuccess: (_data, { team }) => qc.invalidateQueries({ queryKey: ["members", team] }),
  });
}
