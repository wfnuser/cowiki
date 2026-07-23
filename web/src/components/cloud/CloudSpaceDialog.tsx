import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertTriangle, CheckCircle2, Cloud, ExternalLink, GitPullRequest, Loader2, RefreshCw } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Textarea } from '../ui/textarea';
import {
  getCloudStatus,
  linkCloudSpace,
  submitCloud,
  syncCloudIfClean,
  type CloudSyncResult,
} from '../../local-api';
import { createCloudClient } from '../../cloud/client';
import type { CloudSession } from '../../cloud/session';
import { openExternalUrl } from '../../external-links';
import {
  cloudDialogModel,
  cloudPullRequestUrl,
  cloudSpaceUrl,
} from './cloud-space-model';

interface CloudSpaceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  space: { name: string; slug: string } | null;
  session: CloudSession;
  hasLocalChanges: boolean;
  onChanged?: () => void;
}

export function CloudSpaceDialog({
  open,
  onOpenChange,
  space,
  session,
  hasLocalChanges,
  onChanged,
}: CloudSpaceDialogProps) {
  const [status, setStatus] = useState<CloudSyncResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [cloudName, setCloudName] = useState(space?.name || '');
  const [cloudSlug, setCloudSlug] = useState(space?.slug || '');
  const [commitMessage, setCommitMessage] = useState('');
  const [pullRequestTitle, setPullRequestTitle] = useState('');
  const [pullRequestBody, setPullRequestBody] = useState('');
  const client = useMemo(() => createCloudClient(session), [session]);
  const model = status ? cloudDialogModel(status, hasLocalChanges) : null;

  const enrichWithOpenPullRequest = useCallback(async (next: CloudSyncResult) => {
    if (!next.cloudSpaceId || !next.cloudBaseUrl || next.pullRequest) return next;
    try {
      const pullRequests = await client.listPullRequests(next.cloudSpaceId);
      const pullRequest = pullRequests.find((candidate) =>
        candidate.status === 'open' && candidate.headRef === `user/${session.userId}`,
      );
      if (!pullRequest) return next;
      return {
        ...next,
        state: 'submitted' as const,
        pullRequest: {
          id: pullRequest.id,
          number: pullRequest.number,
          title: pullRequest.title,
          headRef: pullRequest.headRef,
          headOid: pullRequest.headOid,
          status: pullRequest.status,
        },
      };
    } catch {
      return next;
    }
  }, [client, session.userId]);

  const loadStatus = useCallback(async () => {
    if (!space) return;
    setLoading(true);
    setError('');
    try {
      setStatus(await enrichWithOpenPullRequest(await getCloudStatus(space.slug)));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [enrichWithOpenPullRequest, space]);

  useEffect(() => {
    if (!open || !space) return;
    setCloudName(space.name);
    setCloudSlug(space.slug);
    setCommitMessage('');
    setPullRequestTitle('');
    setPullRequestBody('');
    void loadStatus();
  }, [loadStatus, open, space]);

  const run = async (operation: () => Promise<CloudSyncResult>) => {
    setLoading(true);
    setError('');
    try {
      const next = await enrichWithOpenPullRequest(await operation());
      setStatus(next);
      onChanged?.();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  };

  const publish = () => {
    if (!space) return;
    void run(() => linkCloudSpace({
      spaceSlug: space.slug,
      cloudBaseUrl: session.baseUrl,
      apiKey: session.apiKey,
      cloudName: cloudName.trim(),
      cloudSlug: cloudSlug.trim(),
      userName: session.userName,
      userId: session.userId,
      commitMessage: commitMessage.trim() || undefined,
    }));
  };

  const submit = () => {
    if (!space) return;
    void run(() => submitCloud({
      spaceSlug: space.slug,
      apiKey: session.apiKey,
      userName: session.userName,
      commitMessage: commitMessage.trim() || undefined,
      pullRequestTitle: pullRequestTitle.trim() || undefined,
      pullRequestBody: pullRequestBody.trim() || undefined,
    }));
  };

  const sync = () => {
    if (!space) return;
    void run(() => syncCloudIfClean(space.slug, session.apiKey));
  };

  const openInBrowser = async () => {
    if (!openUrl) return;
    setError('');
    try {
      await openExternalUrl(openUrl);
    } catch (cause) {
      setError(errorMessage(cause));
    }
  };

  const primaryDisabled = loading
    || !model?.primaryLabel
    || (model.kind === 'publish' && (!cloudName.trim() || !cloudSlug.trim()))
    || (!!model?.commitMessageRequired && !commitMessage.trim());
  const openUrl = status ? cloudPullRequestUrl(status) ?? cloudSpaceUrl(status) : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[540px]">
        <DialogHeader>
          <div className="mb-1 grid size-10 place-items-center rounded-xl bg-accent-soft text-accent"><Cloud size={20} /></div>
          <DialogTitle>{model?.title || 'Cloud Space'}</DialogTitle>
          <DialogDescription>
            Publish is explicit. Your local main remains editable; Cloud main changes only when a maintainer merges a pull request.
          </DialogDescription>
        </DialogHeader>

        {loading && !status ? <div className="flex items-center gap-2 py-8 text-sm text-text-tertiary"><Loader2 className="animate-spin" /> Checking this Space…</div> : null}
        {error && <div className="rounded-lg border border-red/20 bg-red-soft px-4 py-3 text-sm text-red">{error}</div>}

        {model?.kind === 'publish' && (
          <div className="space-y-4">
            <Field label="Cloud Space name"><Input value={cloudName} onChange={(event) => setCloudName(event.target.value)} /></Field>
            <Field label="Cloud slug" hint="Lowercase letters, numbers, and interior hyphens."><Input value={cloudSlug} onChange={(event) => setCloudSlug(slugify(event.target.value))} /></Field>
          </div>
        )}

        {(model?.kind === 'submit' || model?.kind === 'sync-submit' || model?.kind === 'submitted' || model?.kind === 'publish') && model.commitMessageRequired && (
          <Field label="Commit message" hint="CoWiki will commit the current local files before publishing.">
            <Input value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} placeholder="Describe this knowledge update" />
          </Field>
        )}

        {(model?.kind === 'submit' || model?.kind === 'sync-submit' || model?.kind === 'submitted') && (
          <div className="space-y-4">
            <Field label="Pull request title" hint="Optional — the latest commit title is used when left blank."><Input value={pullRequestTitle} onChange={(event) => setPullRequestTitle(event.target.value)} placeholder="Share this update" /></Field>
            <Field label="Description"><Textarea value={pullRequestBody} onChange={(event) => setPullRequestBody(event.target.value)} placeholder="What should reviewers know?" /></Field>
          </div>
        )}

        {model?.kind === 'submitted' && status?.pullRequest && (
          <div className="rounded-xl border bg-secondary p-4">
            <div className="flex items-center gap-2 text-sm font-semibold"><GitPullRequest size={16} className="text-accent" />#{status.pullRequest.number} {status.pullRequest.title}</div>
            <div className="mt-2 font-mono text-[11px] text-text-tertiary">{status.pullRequest.headRef} · {status.pullRequest.headOid.slice(0, 8)}</div>
          </div>
        )}

        {model?.kind === 'attention' && (
          <div className="rounded-xl border border-amber/20 bg-amber-soft p-4 text-sm text-text-secondary">
            <div className="flex items-center gap-2 font-semibold text-amber"><AlertTriangle size={17} />Cloud sync paused without changing your draft</div>
            {status?.state === 'conflicted' ? (
              <>
                <p className="mt-2 leading-6">Resolve the marked files in your local Space, then retry after the working tree is clean.</p>
                {status.conflicts.length > 0 && <ul className="mt-2 list-disc pl-5 font-mono text-xs">{status.conflicts.map((path) => <li key={path}>{path}</li>)}</ul>}
              </>
            ) : (
              <p className="mt-2 leading-6">Your Cloud user branch changed on another device. Review that copy before trying to submit again.</p>
            )}
          </div>
        )}

        {status && !error && !model?.safeStop && model?.kind !== 'submitted' && model?.kind !== 'publish' && (
          <div className="flex items-center gap-2 rounded-lg bg-green-soft px-3 py-2 text-xs text-green"><CheckCircle2 size={14} />{status.message}</div>
        )}

        <DialogFooter className="items-center sm:justify-between">
          <div className="flex gap-2">
            {openUrl && <Button variant="ghost" onClick={() => void openInBrowser()}><ExternalLink /> Open in browser</Button>}
            {status && status.state !== 'unlinked' && !model?.safeStop && <Button variant="outline" disabled={loading || status.state === 'dirty'} onClick={sync}><RefreshCw /> Sync</Button>}
          </div>
          {model?.primaryLabel && (
            <Button disabled={primaryDisabled} onClick={model.kind === 'publish' ? publish : submit}>
              {loading && <Loader2 className="animate-spin" />}{model.primaryLabel}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return <label className="block"><span className="mb-1.5 block text-sm font-medium text-text-secondary">{label}</span>{children}{hint && <span className="mt-1.5 block text-xs text-text-tertiary">{hint}</span>}</label>;
}

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9-]/g, '-').replace(/-+/g, '-').replace(/^-/, '');
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause || 'Cloud operation failed.');
}
