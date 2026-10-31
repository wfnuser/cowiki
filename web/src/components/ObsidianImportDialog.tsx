import { useMemo, useState } from 'react';
import { AlertTriangle, FolderOpen, Info, LoaderCircle } from 'lucide-react';

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { C, fonts } from '@/lib/design';
import {
  formatBytes,
  importFolderName,
  importReportMarkdown,
  importSpaceConfig,
  importWarnings,
  planImport,
} from '@/lib/obsidian-import';
import { importObsidianVault, scanObsidianVault, type ObsidianVaultReport } from '@/local-api';
import { chooseLocalSpaceDirectory, localSpaceIdentityFromPath } from '@/local-space';
import type { Workspace } from '@/api';

interface ObsidianImportDialogProps {
  open: boolean;
  existingSlugs: string[];
  onClose: () => void;
  onImported: (space: Workspace) => void | Promise<void>;
}

function joinPath(parent: string, name: string): string {
  return `${parent.replace(/[\\/]+$/, '')}/${name}`;
}

function uniqueSlug(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base;
  for (let index = 2; ; index += 1) {
    const candidate = `${base}-${index}`;
    if (!taken.includes(candidate)) return candidate;
  }
}

export function ObsidianImportDialog({ open, existingSlugs, onClose, onImported }: ObsidianImportDialogProps) {
  const [report, setReport] = useState<ObsidianVaultReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [parent, setParent] = useState('');
  const [name, setName] = useState('');
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState('');

  const plan = useMemo(() => (report ? planImport(report) : null), [report]);
  const warnings = useMemo(() => (report ? importWarnings(report) : []), [report]);
  const folderName = report ? importFolderName(report.vaultName) : '';
  const destination = parent && folderName ? joinPath(parent, folderName) : '';

  const reset = () => {
    setReport(null);
    setParent('');
    setName('');
    setError('');
  };

  const chooseVault = async () => {
    setError('');
    const path = await chooseLocalSpaceDirectory();
    if (!path) return;
    setScanning(true);
    try {
      const next = await scanObsidianVault(path);
      setReport(next);
      setName(next.vaultName);
      setParent((current) => current || path.replace(/[\\/][^\\/]+[\\/]?$/, ''));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setScanning(false);
    }
  };

  const chooseParent = async () => {
    const path = await chooseLocalSpaceDirectory();
    if (path) setParent(path);
  };

  const runImport = async () => {
    if (!report || !plan || !destination || !name.trim()) return;
    setImporting(true);
    setError('');
    try {
      const slug = uniqueSlug(localSpaceIdentityFromPath(name.trim()).slug, existingSlugs);
      const space = await importObsidianVault({
        name: name.trim(),
        slug,
        sourcePath: report.vaultPath,
        destinationPath: destination,
        spaceConfig: importSpaceConfig(plan, report),
        reportMarkdown: importReportMarkdown(report, plan, new Date(), destination),
      });
      reset();
      await onImported(space);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setImporting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next && !importing) { reset(); onClose(); } }}>
      <DialogContent style={{ maxWidth: 600 }}>
        <DialogHeader>
          <DialogTitle>Import an Obsidian vault</DialogTitle>
        </DialogHeader>
        <p style={{ margin: '-4px 0 4px', color: C.muted, fontSize: 12.5 }}>
          CoWiki reads the vault and works on a copy. Your vault and its Obsidian settings are not changed, so you can keep using both.
        </p>

        {!report ? (
          <Button onClick={() => { void chooseVault(); }} disabled={scanning} style={{ alignSelf: 'flex-start' }}>
            {scanning ? <LoaderCircle className="size-4 animate-spin" /> : <FolderOpen className="size-4" />}
            {scanning ? 'Checking the vault…' : 'Choose vault folder'}
          </Button>
        ) : (
          <>
            <section style={panel}>
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, alignItems: 'baseline' }}>
                <strong style={{ fontSize: 14, color: C.ink }}>{report.vaultName}</strong>
                <button type="button" onClick={() => { void chooseVault(); }} style={linkButton}>Choose another</button>
              </div>
              <div style={statsGrid}>
                <Stat label="Notes" value={report.markdownFiles} />
                <Stat label="Attachments" value={report.attachmentFiles} />
                <Stat label="Folders" value={report.folders} />
                <Stat label="Wikilinks" value={report.wikilinks} />
                <Stat label="Tasks" value={report.taskLines} />
                <Stat label="Size" value={formatBytes(report.totalBytes)} />
              </div>
              {plan && (
                <p style={{ margin: '10px 0 0', fontSize: 12.5, color: C.ink2 }}>
                  Journal: <code style={{ fontFamily: fonts.mono }}>{plan.journal.folder || '(root)'}/{plan.journal.format}</code>
                  {' '}— {plan.journalEntries} entries
                  {plan.journalSource === 'obsidian-settings' ? ' (from Daily Notes settings)' : plan.journalSource === 'detected' ? ' (detected)' : ''}
                  {plan.journal.template ? `, template ${plan.journal.template}` : ''}.
                </p>
              )}
            </section>

            {warnings.length > 0 && (
              <ul style={{ ...panel, listStyle: 'none', margin: 0, display: 'grid', gap: 6, maxHeight: 170, overflow: 'auto' }}>
                {warnings.map((warning) => (
                  <li key={warning.text} style={{ display: 'flex', gap: 8, fontSize: 12, color: C.ink2, lineHeight: 1.45 }}>
                    {warning.level === 'warning'
                      ? <AlertTriangle size={14} color={C.amber} style={{ flexShrink: 0, marginTop: 2 }} />
                      : <Info size={14} color={C.muted} style={{ flexShrink: 0, marginTop: 2 }} />}
                    {warning.text}
                  </li>
                ))}
              </ul>
            )}

            <div style={{ display: 'grid', gridTemplateColumns: '110px 1fr', gap: '8px 12px', alignItems: 'center' }}>
              <label htmlFor="import-name" style={{ fontSize: 12.5, color: C.ink2 }}>Space name</label>
              <Input id="import-name" value={name} onChange={(event) => setName(event.target.value)} />
              <span style={{ fontSize: 12.5, color: C.ink2 }}>Copy to</span>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center', minWidth: 0 }}>
                <code title={destination} style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 11.5, color: C.muted }}>
                  {destination || 'Choose a folder'}
                </code>
                <Button variant="outline" size="sm" onClick={() => { void chooseParent(); }}>Change…</Button>
              </div>
            </div>
            <p style={{ margin: 0, fontSize: 11.5, color: C.muted }}>
              A report with everything found is added to the new Space as “CoWiki Import Report”.
            </p>
          </>
        )}

        {error && <p role="alert" style={{ color: C.red, fontSize: 12.5, margin: 0 }}>{error}</p>}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <Button variant="outline" disabled={importing} onClick={() => { reset(); onClose(); }}>Cancel</Button>
          <Button onClick={() => { void runImport(); }} disabled={!report || !destination || !name.trim() || importing}>
            {importing && <LoaderCircle className="size-4 animate-spin" />}
            {importing ? 'Importing…' : 'Import copy'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Stat({ label, value }: { label: string; value: number | string }) {
  return (
    <div>
      <div style={{ fontSize: 16, fontWeight: 650, color: C.ink }}>{typeof value === 'number' ? value.toLocaleString() : value}</div>
      <div style={{ fontSize: 11, color: C.muted }}>{label}</div>
    </div>
  );
}

const panel: React.CSSProperties = {
  padding: '12px 14px', border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel,
};

const statsGrid: React.CSSProperties = {
  display: 'grid', gridTemplateColumns: 'repeat(6, minmax(0, 1fr))', gap: 10, marginTop: 10,
};

const linkButton: React.CSSProperties = {
  border: 'none', background: 'transparent', color: C.accent, fontSize: 12, cursor: 'pointer', padding: 0,
};
