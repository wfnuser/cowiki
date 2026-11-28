import { useCallback, useEffect, useRef, useState } from 'react';

import {
  discardLocalAgentChange,
  getLocalWorkingDiff,
  keepLocalWorkingDiff,
  listLocalAgentChanges,
  mergeLocalAgentChange,
  type AgentChange,
  type FileDiff,
} from '@/api';
import { Button } from '@/components/ui/button';
import { InlineFeedback } from '@/components/ui/inline-feedback';
import { ViewEmpty, ViewHeader } from '@/components/ui/view-header';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useI18n } from '@/lib/I18nContext';
import { GovernanceReviewPanel } from './GovernanceReviewPanel';
import { DiffView } from './DiffView';
import { agentMergeResult, type LocalReviewSelection } from './local-review-model';

export function LocalReviewDetail({
  workspaceSlug,
  target,
  onDraftChanged,
  onReviewsChanged,
  onAgentChangeClosed,
  onContinueAgent,
  onOpenChange,
}: {
  workspaceSlug: string;
  target: LocalReviewSelection;
  onOpenChange: (id: string) => void;
  onDraftChanged?: () => void;
  onReviewsChanged?: () => void;
  onAgentChangeClosed?: (changeId: string) => void;
  onContinueAgent?: (change: AgentChange) => void;
}) {
  const { t, tl } = useI18n();
  const reviewKey = `${workspaceSlug}:${target.kind === 'local-agent' ? target.changeId : 'draft'}`;
  const [tab, setTab] = useState('changes');
  const [retainedReview, setRetainedReview] = useState<string | null>(null);
  const [draftDiffs, setDraftDiffs] = useState<FileDiff[] | null>(null);
  const [change, setChange] = useState<AgentChange | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState<'commit' | 'merge' | 'discard' | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const onAgentChangeClosedRef = useRef(onAgentChangeClosed);
  useEffect(() => {
    onAgentChangeClosedRef.current = onAgentChangeClosed;
  }, [onAgentChangeClosed]);

  const reload = useCallback(async () => {
    setError(null);
    try {
      if (target.kind === 'local-draft') {
        setDraftDiffs(await getLocalWorkingDiff(workspaceSlug));
        setChange(null);
      } else {
        const changes = await listLocalAgentChanges(workspaceSlug);
        const next = changes.find((candidate) => candidate.id === target.changeId);
        if (!next) {
          onAgentChangeClosedRef.current?.(target.changeId);
          throw new Error('Agent Change no longer exists');
        }
        if (next.status === 'merged' || next.status === 'discarded') {
          onAgentChangeClosedRef.current?.(next.id);
        }
        setChange(next);
        setDraftDiffs(null);
      }
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, [target, workspaceSlug]);

  useEffect(() => {
    const task = window.setTimeout(() => { setNotice(null); void reload(); }, 0);
    return () => window.clearTimeout(task);
  }, [reload]);

  const run = async (
    action: NonNullable<typeof pending>,
    operation: () => Promise<unknown>,
    draftChanged = false,
  ) => {
    if (pending) return;
    setPending(action);
    setError(null);
    setNotice(null);
    try {
      const result = await operation();
      await reload();
      onReviewsChanged?.();
      if (action === 'commit') {
        setNotice(t('Draft committed.'));
      } else if (action === 'merge') {
        const status = (result as AgentChange).status === 'needsResolution' ? 'needsResolution' : 'merged';
        const merge = agentMergeResult(status);
        setError(merge.message);
        if (merge.draftChanged) onDraftChanged?.();
        if (status === 'merged') onAgentChangeClosed?.((result as AgentChange).id);
      } else if (action === 'discard') {
        if (target.kind === 'local-agent') onAgentChangeClosed?.(target.changeId);
      } else if (draftChanged) {
        onDraftChanged?.();
      }
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setPending(null);
    }
  };

  const diffs = target.kind === 'local-draft' ? draftDiffs : change?.diffs;
  const title = target.kind === 'local-draft' ? t('Current Draft') : (change?.title || t('Agent change'));
  const activeChange = change?.status === 'open' || change?.status === 'needsResolution';

  const actions = !loading && !error && target.kind === 'local-draft' && draftDiffs ? (
    <Button
      size="sm"
      disabled={!draftDiffs.length || pending != null}
      title={t('Commit these changes to the current branch')}
      onClick={() => run('commit', () => keepLocalWorkingDiff(workspaceSlug, draftDiffs))}
    >
      {pending === 'commit' ? t('Committing…') : t('Commit draft')}
    </Button>
  ) : !loading && target.kind === 'local-agent' && change && activeChange ? (
    <>
      <Button
        size="sm"
        variant="ghost"
        disabled={pending != null}
        onClick={() => run('discard', () => discardLocalAgentChange(workspaceSlug, change.id))}
      >
        {pending === 'discard' ? t('Discarding…') : t('Discard')}
      </Button>
      {onContinueAgent && (
        <Button size="sm" variant="outline" disabled={pending != null} onClick={() => onContinueAgent(change)}>
          {t('Continue with Agent')}
        </Button>
      )}
      <Button
        size="sm"
        disabled={pending != null}
        onClick={() => run('merge', () => mergeLocalAgentChange(workspaceSlug, change.id), true)}
      >
        {pending === 'merge' ? t('Merging…') : t('Merge into Draft')}
      </Button>
    </>
  ) : undefined;

  return (
    <section style={{ width: 'min(1040px, 100%)' }}>
      <ViewHeader
        className="mb-5"
        title={title}
        actions={tab === 'changes' ? actions : undefined}
        description={(
          <span className="flex items-center gap-2">
            {target.kind === 'local-draft' ? t('Compared with the last commit') : `${t('Agent change')} · ${target.changeId.slice(0, 8)}`}
            {target.kind === 'local-agent' && change && (
              <span className={change.status === 'needsResolution' ? 'font-medium text-red' : 'text-text-tertiary'}>· {tl(statusLabel(change.status))}</span>
            )}
          </span>
        )}
      />

      {error && <InlineFeedback className="mb-4" title={t('Could not update this Review')} description={error} />}
      {notice && <InlineFeedback className="mb-4" tone="neutral" compact title={notice} />}
      <Tabs value={tab} onValueChange={(next) => {
        if (tab === 'governance') setRetainedReview(reviewKey);
        setTab(next);
      }}>
        <TabsList aria-label="Local review">
          <TabsTrigger value="changes">{t('File changes')}</TabsTrigger>
          <TabsTrigger value="governance">{t('Governance')}</TabsTrigger>
        </TabsList>
        <TabsContent value="changes">
          {loading || diffs == null ? (
            <ViewEmpty>{t('Loading changes…')}</ViewEmpty>
          ) : diffs.length ? (
            <DiffView diffs={diffs} />
          ) : (
            <ViewEmpty>{t('No file changes.')}</ViewEmpty>
          )}
        </TabsContent>
        <TabsContent value="governance" forceMount={retainedReview === reviewKey || undefined} hidden={tab !== 'governance'}>
          <GovernanceReviewPanel
            key={reviewKey}
            spaceSlug={workspaceSlug}
            changeId={target.kind === 'local-agent' ? target.changeId : null}
            onStaged={onReviewsChanged}
            onOpenChange={onOpenChange}
          />
        </TabsContent>
      </Tabs>
    </section>
  );
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function statusLabel(status: AgentChange['status']): string {
  switch (status) {
    case 'needsResolution': return 'Needs resolution';
    case 'merged': return 'Merged';
    case 'discarded': return 'Discarded';
    default: return 'Open';
  }
}
