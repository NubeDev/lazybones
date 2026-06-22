import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  CircleDot,
  ExternalLink,
  FolderGit2,
  GitBranch,
  GitPullRequest,
  Rocket,
  Save,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { RepoPicker } from "@/features/workflows/repo-picker";
import { BranchField } from "@/features/workflows/branch-field";
import { ApiError } from "@/lib/api/client";
import {
  createDocBranch,
  createDocIssue,
  createDocPr,
  publishDoc,
  setDocRepo,
} from "@/lib/api/documents";
import type { Document } from "@/types/document";

/** The Repository / Publish panel for a document.
 *
 *  Set a GitHub target (local checkout path + base branch + output path), then
 *  branch → commit the rendered doc → open a PR/issue via the existing
 *  `lazybones-gh` wrapper — or one-click **Publish** (branch → commit → PR). The
 *  resulting branch/issue/PR links are persisted on the document and shown here.
 *  Modeled on the workflow issues/PRs panels. */
export function DocumentRepo({ document }: { document: Document }) {
  const qc = useQueryClient();
  const id = document.id;
  const repo = document.repo ?? null;

  const [path, setPath] = useState(repo?.repo ?? "");
  const [baseBranch, setBaseBranch] = useState<string | null>(repo?.base_branch ?? null);
  const [branchPrefix, setBranchPrefix] = useState(repo?.branch_prefix ?? "");
  const [outputPath, setOutputPath] = useState(repo?.output_path ?? `docs/${id}.md`);

  // Re-seed when the document's persisted repo changes (e.g. after a gh action).
  useEffect(() => {
    if (repo) {
      setPath(repo.repo);
      setBaseBranch(repo.base_branch ?? null);
      setBranchPrefix(repo.branch_prefix ?? "");
      setOutputPath(repo.output_path);
    }
  }, [repo]);

  const invalidate = () => qc.invalidateQueries({ queryKey: ["document", id] });

  const saveTarget = useMutation({
    mutationFn: () =>
      setDocRepo(id, {
        repo: path.trim(),
        base_branch: baseBranch,
        branch_prefix: branchPrefix.trim() || null,
        output_path: outputPath.trim(),
      }),
    onSuccess: invalidate,
  });

  const branch = useMutation({ mutationFn: () => createDocBranch(id), onSuccess: invalidate });
  const issue = useMutation({ mutationFn: () => createDocIssue(id), onSuccess: invalidate });
  const pr = useMutation({ mutationFn: () => createDocPr(id), onSuccess: invalidate });
  const publish = useMutation({ mutationFn: () => publishDoc(id), onSuccess: invalidate });

  const targetSet = !!repo?.repo;
  const canSave = !!path.trim() && !!outputPath.trim() && !saveTarget.isPending;

  const busy =
    branch.isPending || issue.isPending || pr.isPending || publish.isPending;
  const actionError = [branch, issue, pr, publish, saveTarget].find((m) => m.error)?.error;

  return (
    <div className="space-y-4">
      {/* Repo target form. */}
      <div className="space-y-3 rounded-md border border-border bg-surface-2/40 p-3">
        <p className="flex items-center gap-1.5 text-xs font-medium">
          <FolderGit2 className="size-3.5" /> Repository target
        </p>

        <Field label="Local checkout">
          <div className="flex gap-2">
            <Input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/path/to/repo"
              className="font-mono text-xs"
            />
            <RepoPicker
              trigger={
                <Button size="sm" variant="secondary">
                  Browse
                </Button>
              }
              onPick={setPath}
            />
          </div>
        </Field>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field label="Base branch">
            <BranchField dir={path || null} value={baseBranch} onChange={setBaseBranch} />
          </Field>
          <Field label="Branch prefix" hint="defaults to doc/">
            <Input
              value={branchPrefix}
              onChange={(e) => setBranchPrefix(e.target.value)}
              placeholder="doc/"
              className="font-mono text-xs"
            />
          </Field>
        </div>

        <Field label="Output path" hint="where the rendered doc is committed">
          <Input
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            placeholder={`docs/${id}.md`}
            className="font-mono text-xs"
          />
        </Field>

        <div className="flex justify-end">
          <Button size="sm" onClick={() => saveTarget.mutate()} disabled={!canSave}>
            <Save /> {targetSet ? "Update target" : "Save target"}
          </Button>
        </div>
      </div>

      {/* Publishing actions. */}
      <div className="space-y-3 rounded-md border border-border bg-surface-2/40 p-3">
        <p className="text-xs font-medium">Publish</p>
        {!targetSet ? (
          <p className="text-[11px] text-muted-foreground">
            Set a repository target first to enable publishing.
          </p>
        ) : (
          <>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="secondary" onClick={() => branch.mutate()} disabled={busy}>
                <GitBranch /> Create branch
              </Button>
              <Button size="sm" variant="secondary" onClick={() => issue.mutate()} disabled={busy}>
                <CircleDot /> Create issue
              </Button>
              <Button size="sm" variant="secondary" onClick={() => pr.mutate()} disabled={busy}>
                <GitPullRequest /> Open PR
              </Button>
              <Button size="sm" onClick={() => publish.mutate()} disabled={busy} title="Branch → commit → PR">
                <Rocket /> {publish.isPending ? "Publishing…" : "Publish"}
              </Button>
            </div>
            <p className="text-[10px] text-muted-foreground">
              Publish branches off the base, renders + commits the doc to the output path, pushes,
              and opens a PR in one step.
            </p>
          </>
        )}

        {actionError && (
          <p className="rounded-md border border-status-blocked/30 bg-status-blocked/10 px-3 py-2 text-xs text-status-blocked">
            {actionError instanceof ApiError ? actionError.message : "The GitHub action failed."}
          </p>
        )}

        {/* Resulting linkage. */}
        {repo && (repo.branch || repo.issue_url || repo.pr_url) && (
          <div className="space-y-1.5 border-t border-border pt-2">
            {repo.branch && (
              <p className="flex items-center gap-1.5 text-[11px]">
                <GitBranch className="size-3.5 text-muted-foreground" />
                <span className="font-mono">{repo.branch}</span>
              </p>
            )}
            {repo.issue_url && <LinkLine icon={CircleDot} label="Issue" url={repo.issue_url} />}
            {repo.pr_url && <LinkLine icon={GitPullRequest} label="Pull request" url={repo.pr_url} />}
          </div>
        )}
      </div>
    </div>
  );
}

function LinkLine({
  icon: Icon,
  label,
  url,
}: {
  icon: typeof CircleDot;
  label: string;
  url: string;
}) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className="flex items-center gap-1.5 text-[11px] text-accent hover:underline"
    >
      <Icon className="size-3.5" /> {label}
      <ExternalLink className="size-3" />
    </a>
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
